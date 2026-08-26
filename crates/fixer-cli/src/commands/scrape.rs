use crate::{
    AppError, AppResult, RunStatus,
    args::{MediaKindArg, PlacementArg, PlanArgs, ScrapeArgs},
    config::{Config, ConflictPolicy, OutputPreset},
    json::PlanDto,
    render,
};
use fixer_core::{AssetKind, LocalizedValue, Movie, MovieRelease, ReleaseDate, ReleaseId, WorkId};
use fixer_provider_local::{
    EpisodeHint, LocalProvider, MediaHint, ScanWarning, identify_episode_path, identify_path,
    parse_matroska_tags, scan, scan_anime, scan_books, scan_music, scan_television,
};
use fixer_sdk::output::{ExecutionPolicy, OutputPlanExt, PlacementMode, plan_media_placement};
use fixer_writer_local::{
    AnimeWriter, BookWriter, JsonWriter, MusicWriter, PathTemplate, TelevisionWriter,
    TemplateContext,
};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy)]
enum OutputMode {
    Scrape,
    Plan { json: bool, kind: MediaKindArg },
}

#[derive(Debug, Clone, Copy)]
struct FinalizationPolicy {
    output_preset: OutputPreset,
    conflict_policy: ConflictPolicy,
    conflicts: usize,
}

impl FinalizationPolicy {
    const fn new(
        output_preset: OutputPreset,
        conflict_policy: ConflictPolicy,
        conflicts: usize,
    ) -> Self {
        Self {
            output_preset,
            conflict_policy,
            conflicts,
        }
    }

    const fn requires_review(self) -> bool {
        self.conflicts > 0 && matches!(self.conflict_policy, ConflictPolicy::Review)
    }

    const fn rejects(self) -> bool {
        self.conflicts > 0 && matches!(self.conflict_policy, ConflictPolicy::Error)
    }
}

pub async fn run(args: ScrapeArgs, config: &Config) -> AppResult<RunStatus> {
    run_with_mode(args, config, OutputMode::Scrape).await
}

pub async fn plan(args: PlanArgs, config: &Config) -> AppResult<RunStatus> {
    let mode = OutputMode::Plan {
        json: args.json,
        kind: args.kind,
    };
    run_with_mode(
        ScrapeArgs {
            path: args.path,
            kind: args.kind,
            dry_run: true,
            apply: false,
            placement: args.placement,
            update_epub: false,
        },
        config,
        mode,
    )
    .await
}

async fn run_with_mode(
    mut args: ScrapeArgs,
    config: &Config,
    mode: OutputMode,
) -> AppResult<RunStatus> {
    args.placement
        .get_or_insert_with(|| config.placement.into());
    if !args.path.exists() {
        return Err(AppError::new(format!(
            "input path does not exist: {}",
            args.path.display()
        )));
    }
    if args.update_epub && args.kind != MediaKindArg::Book {
        return Err(AppError::new(
            "--update-epub is supported only for book scrape",
        ));
    }
    match args.kind {
        MediaKindArg::Anime => scrape_anime(args, config, mode).await,
        MediaKindArg::Book => scrape_book(args, config, mode).await,
        MediaKindArg::Movie => scrape_movie(args, config, mode).await,
        MediaKindArg::Music => scrape_music(args, config, mode).await,
        MediaKindArg::Television => scrape_television(args, config, mode).await,
    }
}

async fn scrape_anime(args: ScrapeArgs, config: &Config, mode: OutputMode) -> AppResult<RunStatus> {
    if args.placement() != PlacementArg::InPlace {
        return Err(AppError::new(
            "anime scrape currently supports only in-place placement",
        ));
    }
    let scan_root = scan_root(&args.path)?;
    let result = scan_anime(scan_root).map_err(AppError::new)?;
    if result.documents.is_empty() {
        return Err(AppError::new("no local anime metadata was found"));
    }
    if result.documents.len() != 1 {
        return Err(AppError::new(format!(
            "ambiguous anime input: found {} series; scrape one series at a time",
            result.documents.len()
        )));
    }
    let anime = &result.documents[0];
    let title = anime
        .titles
        .entries()
        .first()
        .map(|entry| entry.value().clone())
        .ok_or_else(|| AppError::new("local anime series has no title"))?;
    let provider = LocalProvider::from_anime_documents(result.documents).map_err(AppError::new)?;
    let fixer = super::build_fixer(provider, config)?;
    let resolved = fixer.anime(title).resolve().await.map_err(AppError::new)?;
    let output_root = &result.roots[0];
    let conflicts = resolved.conflicts.len();
    let plan = AnimeWriter
        .plan_resolved(&resolved, output_root)
        .map_err(AppError::new)?;
    finish_plan(
        plan,
        &args,
        output_root,
        &result.warnings,
        None,
        mode,
        FinalizationPolicy::new(config.output_preset, config.conflict_policy, conflicts),
    )
}

async fn scrape_book(args: ScrapeArgs, config: &Config, mode: OutputMode) -> AppResult<RunStatus> {
    if args.placement() != PlacementArg::InPlace {
        return Err(AppError::new(
            "book scrape currently supports only in-place placement",
        ));
    }
    let scan_root = scan_root(&args.path)?;
    let result = scan_books(scan_root).map_err(AppError::new)?;
    if result.documents.is_empty() {
        return Err(AppError::new("no local EPUB metadata was found"));
    }
    if result.documents.len() != 1 {
        return Err(AppError::new(format!(
            "ambiguous book input: found {} works; scrape one work at a time",
            result.documents.len()
        )));
    }
    let work = &result.documents[0];
    let selected_edition = if args.path.is_file() {
        let input = args.path.to_string_lossy();
        work.editions.iter().find(|edition| {
            edition.assets.iter().any(|asset| {
                asset.kind == AssetKind::BookFile && asset.source_path.as_str() == input
            })
        })
    } else if work.editions.len() == 1 {
        work.editions.first()
    } else {
        return Err(AppError::new(
            "book directory contains multiple editions; pass one EPUB path",
        ));
    }
    .ok_or_else(|| AppError::new("input EPUB does not match a scanned edition"))?;
    let isbn = selected_edition.isbn_13.clone();
    let title = work
        .titles
        .entries()
        .first()
        .map(|entry| entry.value().clone())
        .ok_or_else(|| AppError::new("local book work has no title"))?;
    let output_root = result.roots[0].clone();
    let warnings = result.warnings;
    let provider = LocalProvider::from_book_documents(result.documents).map_err(AppError::new)?;
    let fixer = super::build_fixer(provider, config)?;
    let resolved = fixer
        .book(title)
        .isbn(isbn.clone())
        .resolve()
        .await
        .map_err(AppError::new)?;
    let mut writer = BookWriter::for_isbn(isbn);
    if args.update_epub {
        if !args.path.is_file() {
            return Err(AppError::new("--update-epub requires one EPUB file path"));
        }
        writer = writer.with_epub_mutation_target(args.path.clone());
    }
    let conflicts = resolved.conflicts.len();
    let plan = writer
        .plan_resolved(&resolved, &output_root)
        .map_err(AppError::new)?;
    finish_plan(
        plan,
        &args,
        &output_root,
        &warnings,
        None,
        mode,
        FinalizationPolicy::new(config.output_preset, config.conflict_policy, conflicts),
    )
}

async fn scrape_movie(args: ScrapeArgs, config: &Config, mode: OutputMode) -> AppResult<RunStatus> {
    let scan_root = scan_root(&args.path)?;
    let mut result = scan(scan_root).map_err(AppError::new)?;
    result
        .documents
        .sort_by(|left, right| left.id.as_str().cmp(right.id.as_str()));
    let hint = identify_path(&args.path).ok();
    if result.documents.is_empty() {
        let hint = hint
            .clone()
            .ok_or_else(|| AppError::new("no local movie metadata or filename hint was found"))?;
        result.documents.push(movie_from_hint(hint)?);
    }
    let (query_title, query_year) = movie_query_for(&args.path, hint.as_ref(), &result.documents)?;
    let provider = LocalProvider::from_documents(result.documents).map_err(AppError::new)?;
    let fixer = super::build_fixer(provider, config)?;
    let mut query = fixer.movie(query_title);
    if let Some(year) = query_year {
        query = query.year(year);
    }
    let resolved = query.resolve().await.map_err(AppError::new)?;
    let output_root = movie_output_root(&args.path, args.placement(), &resolved)?;
    let conflicts = resolved.conflicts.len();
    let plan = JsonWriter
        .plan_resolved(&resolved, &output_root)
        .map_err(AppError::new)?;
    finish_plan(
        plan,
        &args,
        &output_root,
        &result.warnings,
        None,
        mode,
        FinalizationPolicy::new(config.output_preset, config.conflict_policy, conflicts),
    )
}

async fn scrape_music(args: ScrapeArgs, config: &Config, mode: OutputMode) -> AppResult<RunStatus> {
    if args.placement() != PlacementArg::InPlace {
        return Err(AppError::new(
            "music scrape currently supports only in-place placement",
        ));
    }
    let scan_root = scan_root(&args.path)?;
    let result = scan_music(scan_root).map_err(AppError::new)?;
    if result.documents.is_empty() {
        return Err(AppError::new("no local music metadata was found"));
    }
    if result.documents.len() != 1 {
        return Err(AppError::new(format!(
            "ambiguous music input: found {} albums; scrape one album at a time",
            result.documents.len()
        )));
    }
    let title = result.documents[0]
        .titles
        .entries()
        .first()
        .map(|entry| entry.value().clone())
        .ok_or_else(|| AppError::new("local music album has no title"))?;
    let output_root = result.roots[0].clone();
    let warnings = result.warnings;
    let provider = LocalProvider::from_music_documents(result.documents).map_err(AppError::new)?;
    let fixer = super::build_fixer(provider, config)?;
    let resolved = fixer.music(title).resolve().await.map_err(AppError::new)?;
    let conflicts = resolved.conflicts.len();
    let plan = MusicWriter::default()
        .plan_resolved(&resolved, &output_root)
        .map_err(AppError::new)?;
    finish_plan(
        plan,
        &args,
        &output_root,
        &warnings,
        None,
        mode,
        FinalizationPolicy::new(config.output_preset, config.conflict_policy, conflicts),
    )
}

async fn scrape_television(
    args: ScrapeArgs,
    config: &Config,
    mode: OutputMode,
) -> AppResult<RunStatus> {
    let scan_root = scan_root(&args.path)?;
    let result = scan_television(scan_root).map_err(AppError::new)?;
    if result.documents.is_empty() {
        return Err(AppError::new("no local television episodes were found"));
    }
    if result.documents.len() != 1 {
        return Err(AppError::new(format!(
            "ambiguous television input: found {} series; scrape one series at a time",
            result.documents.len()
        )));
    }
    let series = &result.documents[0];
    let series_root = &result.roots[0];
    let hint = args
        .path
        .is_file()
        .then(|| identify_episode_path(&args.path).ok())
        .flatten();
    let title = hint
        .as_ref()
        .map(|hint| hint.series_title.clone())
        .or_else(|| {
            series
                .titles
                .entries()
                .first()
                .map(|entry| entry.value().clone())
        })
        .ok_or_else(|| AppError::new("local television series has no title"))?;
    let ordering = series.ordering;
    let placement_target = (args.placement() != PlacementArg::InPlace)
        .then(|| television_placement_target(&args.path, series, hint.as_ref()))
        .transpose()?;
    let (provider, warnings) = LocalProvider::from_scan(scan_root).map_err(AppError::new)?;
    let fixer = super::build_fixer(provider, config)?;
    let mut query = fixer.television(title).ordering(ordering);
    if let Some(EpisodeHint { external_ids, .. }) = &hint {
        for external_id in external_ids {
            query = query.external_id(external_id.clone());
        }
    }
    let resolved = query.resolve().await.map_err(AppError::new)?;
    let output_root = television_output_root(series_root, args.placement(), &resolved)?;
    let conflicts = resolved.conflicts.len();
    let plan = TelevisionWriter
        .plan_resolved(&resolved, &output_root)
        .map_err(AppError::new)?;
    finish_plan(
        plan,
        &args,
        &output_root,
        &warnings,
        placement_target.as_deref(),
        mode,
        FinalizationPolicy::new(config.output_preset, config.conflict_policy, conflicts),
    )
}

fn finish_plan(
    mut plan: fixer_core::OutputPlan,
    args: &ScrapeArgs,
    output_root: &Path,
    warnings: &[ScanWarning],
    placement_target: Option<&Path>,
    mode: OutputMode,
    policy: FinalizationPolicy,
) -> AppResult<RunStatus> {
    plan = apply_output_preset(plan, policy.output_preset)?;
    if args.placement() != PlacementArg::InPlace {
        if !args.path.is_file() {
            return Err(AppError::new(
                "non-in-place placement requires a media file path",
            ));
        }
        let target = placement_target.map(PathBuf::from).unwrap_or_else(|| {
            PathBuf::from(args.path.file_name().expect("file path was checked above"))
        });
        if output_root.join(&target) != args.path {
            let placement = plan_media_placement(
                &args.path,
                output_root,
                target,
                placement_mode(args.placement()),
            )
            .map_err(AppError::new)?;
            for operation in placement.operations() {
                plan.push(operation.clone());
            }
        }
    }
    if policy.rejects() {
        return Err(AppError::new(format!(
            "conflict policy rejected {} metadata conflict(s)",
            policy.conflicts
        )));
    }

    if let OutputMode::Plan { json, kind } = mode {
        if json {
            render::json(&PlanDto::new(kind.as_str(), output_root, &plan))?;
        } else {
            println!(
                "planned {} operation(s) at {}",
                plan.operations().len(),
                output_root.display()
            );
        }
        if policy.requires_review() {
            eprintln!("review required: {} metadata conflict(s)", policy.conflicts);
            return Ok(RunStatus::ReviewRequired);
        }
        return Ok(super::finish_with_warnings(warnings));
    }

    if policy.requires_review() {
        eprintln!("review required: {} metadata conflict(s)", policy.conflicts);
        return Ok(RunStatus::ReviewRequired);
    }

    let dry_run = args.dry_run || !args.apply;
    let policy = if dry_run {
        ExecutionPolicy::dry_run()
    } else {
        ExecutionPolicy::default()
    };
    let report = plan.execute(policy).map_err(AppError::new)?;
    println!(
        "{} {} operation(s) at {}",
        if dry_run { "planned" } else { "executed" },
        report.operations().len(),
        output_root.display()
    );
    Ok(super::finish_with_warnings(warnings))
}

fn scan_root(path: &Path) -> AppResult<&Path> {
    if path.is_dir() {
        Ok(path)
    } else {
        path.parent()
            .ok_or_else(|| AppError::new("input path has no parent directory"))
    }
}

fn movie_query_for(
    path: &Path,
    hint: Option<&MediaHint>,
    documents: &[Movie],
) -> AppResult<(String, Option<u16>)> {
    if path.is_file() {
        if let Some(hint) = hint {
            return Ok((hint.title.clone(), hint.year));
        }
    }
    let movie = documents
        .first()
        .ok_or_else(|| AppError::new("no local movie document was found"))?;
    let title = movie
        .titles
        .entries()
        .first()
        .map(|entry| entry.value().clone())
        .ok_or_else(|| AppError::new("local movie has no title"))?;
    Ok((title, movie.release_year()))
}

fn movie_from_hint(hint: MediaHint) -> AppResult<Movie> {
    let slug = hint
        .title
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    let slug = if slug.is_empty() { "movie" } else { &slug };
    let mut titles = LocalizedValue::new();
    titles.insert("und", hint.title).map_err(AppError::new)?;
    let mut movie = Movie::new(
        WorkId::new(format!("local-{slug}")).map_err(AppError::new)?,
        titles,
    );
    if let Some(year) = hint.year {
        movie.releases.push(MovieRelease::new(
            ReleaseId::new(format!("local-{slug}-{year}")).map_err(AppError::new)?,
            ReleaseDate::year(year).map_err(AppError::new)?,
        ));
    }
    Ok(movie)
}

fn television_placement_target(
    path: &Path,
    series: &fixer_core::Series,
    hint: Option<&EpisodeHint>,
) -> AppResult<PathBuf> {
    let file_name = path
        .file_name()
        .ok_or_else(|| AppError::new("media path has no file name"))?;
    let season = if series.ordering == fixer_core::OrderingScheme::Absolute {
        series.seasons.first().map(|season| season.number)
    } else {
        tagged_episode_season(path)
            .or_else(|| hint.and_then(|hint| hint.sequence.season))
            .or_else(|| series.seasons.first().map(|season| season.number))
    }
    .unwrap_or_default();
    Ok(PathBuf::from(format!("Season {season:02}")).join(file_name))
}

fn tagged_episode_season(path: &Path) -> Option<u32> {
    std::fs::read_to_string(path.with_extension("tags.xml"))
        .ok()
        .and_then(|input| parse_matroska_tags(&input).ok())
        .and_then(|tags| tags.season)
}

fn movie_output_root(
    path: &Path,
    placement: PlacementArg,
    resolved: &fixer_core::Resolved<Movie>,
) -> AppResult<PathBuf> {
    let base = scan_root(path)?;
    if placement == PlacementArg::InPlace {
        return Ok(base.to_path_buf());
    }
    let context =
        TemplateContext::movie(resolved, ["zh-CN", "en", "und"]).map_err(AppError::new)?;
    let source = if resolved.value.release_year().is_some() {
        "{{ title | sanitize }} ({{ year }})"
    } else {
        "{{ title | sanitize }}"
    };
    let folder = PathTemplate::new(source)
        .and_then(|template| template.render(&context))
        .map_err(AppError::new)?;
    Ok(base.join(folder))
}

fn television_output_root(
    series_root: &Path,
    placement: PlacementArg,
    resolved: &fixer_core::Resolved<fixer_core::Series>,
) -> AppResult<PathBuf> {
    if placement == PlacementArg::InPlace {
        return Ok(series_root.to_path_buf());
    }
    let base = series_root.parent().unwrap_or(series_root);
    let title = resolved
        .value
        .titles
        .entries()
        .first()
        .map(|entry| entry.value().as_str())
        .unwrap_or("television");
    Ok(base.join(safe_folder_name(title)))
}

fn safe_folder_name(value: &str) -> String {
    let cleaned = value
        .chars()
        .map(|character| {
            if character.is_control()
                || matches!(
                    character,
                    '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|'
                )
            {
                ' '
            } else {
                character
            }
        })
        .collect::<String>();
    let cleaned = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    let cleaned = cleaned.trim_matches([' ', '.']);
    if cleaned.is_empty() {
        "television".to_owned()
    } else {
        cleaned.to_owned()
    }
}

fn apply_output_preset(
    plan: fixer_core::OutputPlan,
    preset: OutputPreset,
) -> AppResult<fixer_core::OutputPlan> {
    if preset == OutputPreset::Full {
        return Ok(plan);
    }
    let dropped_targets = plan
        .operations()
        .iter()
        .filter(|operation| {
            !matches!(
                operation,
                fixer_core::OutputOperation::CreateDirectory { .. }
                    | fixer_core::OutputOperation::WriteBytes { .. }
            )
        })
        .filter_map(fixer_core::OutputOperation::target)
        .map(Path::to_path_buf)
        .collect::<Vec<_>>();
    let mut filtered = fixer_core::OutputPlan::new(plan.output_root.clone());
    for operation in plan.operations() {
        match operation {
            fixer_core::OutputOperation::CreateDirectory { .. } => filtered.push(operation.clone()),
            fixer_core::OutputOperation::WriteBytes { target, content } => {
                if target.file_name().and_then(|name| name.to_str()) == Some("fixer-manifest.json")
                {
                    filtered.push(reconcile_manifest(target, content, &dropped_targets)?);
                } else {
                    filtered.push(operation.clone());
                }
            }
            fixer_core::OutputOperation::Copy { .. }
            | fixer_core::OutputOperation::Symlink { .. }
            | fixer_core::OutputOperation::Hardlink { .. }
            | fixer_core::OutputOperation::Reflink { .. } => {}
        }
    }
    Ok(filtered)
}

fn reconcile_manifest(
    target: &Path,
    content: &fixer_core::PlannedContent,
    dropped_targets: &[PathBuf],
) -> AppResult<fixer_core::OutputOperation> {
    let mut manifest: serde_json::Value = serde_json::from_slice(content.as_bytes())
        .map_err(|error| AppError::new(format!("invalid planned manifest: {error}")))?;
    if let Some(planned_files) = manifest.get_mut("planned_files") {
        match planned_files {
            serde_json::Value::Array(files) => files.retain(|file| {
                file.as_str().is_none_or(|file| {
                    !dropped_targets
                        .iter()
                        .any(|target| target == Path::new(file))
                })
            }),
            serde_json::Value::Object(files) => files.retain(|_, file| {
                file.as_str().is_none_or(|file| {
                    !dropped_targets
                        .iter()
                        .any(|target| target == Path::new(file))
                })
            }),
            _ => {}
        }
    }
    let mut bytes = serde_json::to_vec_pretty(&manifest)
        .map_err(|error| AppError::new(format!("could not serialize planned manifest: {error}")))?;
    bytes.push(b'\n');
    fixer_core::OutputOperation::write_bytes(
        target.to_path_buf(),
        fixer_core::PlannedContent::new(bytes),
    )
    .map_err(AppError::new)
}

const fn placement_mode(placement: PlacementArg) -> PlacementMode {
    match placement {
        PlacementArg::InPlace => PlacementMode::InPlace,
        PlacementArg::Symlink => PlacementMode::RelativeSymlink,
        PlacementArg::Hardlink => PlacementMode::Hardlink,
        PlacementArg::Copy => PlacementMode::Copy,
        PlacementArg::Reflink => PlacementMode::Reflink,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fixer_core::{OutputOperation, OutputPlan, PlannedContent};

    #[test]
    fn output_preset_filters_writer_asset_transfers() {
        let mut plan = OutputPlan::new("library");
        plan.push(OutputOperation::create_directory("metadata").unwrap());
        plan.push(
            OutputOperation::write_bytes("metadata/item.json", PlannedContent::new(b"{}")).unwrap(),
        );
        plan.push(
            OutputOperation::write_bytes(
                "fixer-manifest.json",
                PlannedContent::new(br#"{"planned_files":["metadata/item.json","cover.jpg"]}"#),
            )
            .unwrap(),
        );
        plan.push(OutputOperation::copy("source.jpg", "cover.jpg").unwrap());
        plan.push(OutputOperation::symlink("source.mkv", "movie.mkv").unwrap());
        plan.push(OutputOperation::hardlink("source.flac", "track.flac").unwrap());
        plan.push(OutputOperation::reflink("source.epub", "book.epub").unwrap());

        let metadata = apply_output_preset(plan.clone(), OutputPreset::Metadata).unwrap();
        assert_eq!(metadata.operations().len(), 3);
        assert!(metadata.operations().iter().all(|operation| matches!(
            operation,
            OutputOperation::CreateDirectory { .. } | OutputOperation::WriteBytes { .. }
        )));
        let manifest = metadata
            .operations()
            .iter()
            .find_map(|operation| match operation {
                OutputOperation::WriteBytes { target, content }
                    if target == std::path::Path::new("fixer-manifest.json") =>
                {
                    Some(serde_json::from_slice::<serde_json::Value>(content.as_bytes()).unwrap())
                }
                _ => None,
            })
            .unwrap();
        assert_eq!(
            manifest["planned_files"],
            serde_json::json!(["metadata/item.json"])
        );

        let full = apply_output_preset(plan.clone(), OutputPreset::Full).unwrap();
        assert_eq!(full, plan);
    }

    #[test]
    fn output_preset_reconciles_named_manifest_entries() {
        let mut plan = OutputPlan::new("library");
        plan.push(
            OutputOperation::write_bytes(
                "fixer-manifest.json",
                PlannedContent::new(
                    br#"{"planned_files":{"metadata":"item.json","artwork":"cover.jpg"}}"#,
                ),
            )
            .unwrap(),
        );
        plan.push(OutputOperation::copy("source.jpg", "cover.jpg").unwrap());

        let metadata = apply_output_preset(plan, OutputPreset::Metadata).unwrap();
        let OutputOperation::WriteBytes { content, .. } = &metadata.operations()[0] else {
            panic!("expected manifest write");
        };
        let manifest: serde_json::Value = serde_json::from_slice(content.as_bytes()).unwrap();
        assert_eq!(
            manifest["planned_files"],
            serde_json::json!({"metadata":"item.json"})
        );
    }
}
