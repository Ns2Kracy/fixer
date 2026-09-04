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
use fixer_sdk::output::{ExecutionPolicy, OutputPlanExt};
use fixer_writer_local::{
    AnimeWriter, BookWriter, JsonWriter, MusicWriter, OrganizationMedia, OrganizationPlacement,
    OrganizationRequest, TelevisionWriter, metadata_only, organize,
};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy)]
enum OutputMode {
    Scrape,
    Plan { json: bool, kind: MediaKindArg },
}

#[derive(Debug, Clone, Copy)]
struct PlanDiagnostics<'a> {
    scan: &'a [ScanWarning],
    resolution: &'a [fixer_core::ResolutionWarning],
}

impl<'a> PlanDiagnostics<'a> {
    const fn new(scan: &'a [ScanWarning], resolution: &'a [fixer_core::ResolutionWarning]) -> Self {
        Self { scan, resolution }
    }
}

#[derive(Debug, Clone, Copy)]
struct FinalizationPolicy {
    conflict_policy: ConflictPolicy,
    conflicts: usize,
}

impl FinalizationPolicy {
    const fn new(conflict_policy: ConflictPolicy, conflicts: usize) -> Self {
        Self {
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
    args: ScrapeArgs,
    config: &Config,
    mode: OutputMode,
) -> AppResult<RunStatus> {
    if !args.path.exists() {
        return Err(AppError::invalid_input(format!(
            "input path does not exist: {}",
            args.path.display()
        )));
    }
    if args.update_epub && args.kind != MediaKindArg::Book {
        return Err(AppError::invalid_input(
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
        return Err(AppError::invalid_input(
            "anime scrape currently supports only in-place placement",
        ));
    }
    let scan_root = scan_root(&args.path)?;
    let result = scan_anime(scan_root).map_err(AppError::new)?;
    if result.documents.is_empty() {
        return Err(AppError::invalid_input("no local anime metadata was found"));
    }
    if result.documents.len() != 1 {
        return Err(AppError::invalid_input(format!(
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
        .ok_or_else(|| AppError::invalid_input("local anime series has no title"))?;
    let provider = LocalProvider::from_anime_documents(result.documents).map_err(AppError::new)?;
    let fixer = super::build_fixer(provider, config)?;
    let resolved = fixer.anime(title).resolve().await.map_err(AppError::new)?;
    let output_root = &result.roots[0];
    let conflicts = resolved.conflicts.len();
    let plan = AnimeWriter
        .plan_resolved(&resolved, output_root)
        .map_err(AppError::new)?;
    let plan = apply_output_preset(plan, config.output_preset)?;
    finish_plan(
        plan,
        &args,
        output_root,
        PlanDiagnostics::new(&result.warnings, &resolved.warnings),
        mode,
        FinalizationPolicy::new(config.conflict_policy, conflicts),
    )
}

async fn scrape_book(args: ScrapeArgs, config: &Config, mode: OutputMode) -> AppResult<RunStatus> {
    if args.placement() != PlacementArg::InPlace {
        return Err(AppError::invalid_input(
            "book scrape currently supports only in-place placement",
        ));
    }
    let scan_root = scan_root(&args.path)?;
    let result = scan_books(scan_root).map_err(AppError::new)?;
    if result.documents.is_empty() {
        return Err(AppError::invalid_input("no local EPUB metadata was found"));
    }
    if result.documents.len() != 1 {
        return Err(AppError::invalid_input(format!(
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
        return Err(AppError::invalid_input(
            "book directory contains multiple editions; pass one EPUB path",
        ));
    }
    .ok_or_else(|| AppError::invalid_input("input EPUB does not match a scanned edition"))?;
    let isbn = selected_edition.isbn_13.clone();
    let title = work
        .titles
        .entries()
        .first()
        .map(|entry| entry.value().clone())
        .ok_or_else(|| AppError::invalid_input("local book work has no title"))?;
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
            return Err(AppError::invalid_input(
                "--update-epub requires one EPUB file path",
            ));
        }
        writer = writer.with_epub_mutation_target(args.path.clone());
    }
    let conflicts = resolved.conflicts.len();
    let plan = writer
        .plan_resolved(&resolved, &output_root)
        .map_err(AppError::new)?;
    let plan = apply_output_preset(plan, config.output_preset)?;
    finish_plan(
        plan,
        &args,
        &output_root,
        PlanDiagnostics::new(&warnings, &resolved.warnings),
        mode,
        FinalizationPolicy::new(config.conflict_policy, conflicts),
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
        let hint = hint.clone().ok_or_else(|| {
            AppError::invalid_input("no local movie metadata or filename hint was found")
        })?;
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
    let output_root = scan_root.to_path_buf();
    let conflicts = resolved.conflicts.len();
    let plan = JsonWriter
        .plan_resolved(&resolved, &output_root)
        .map_err(AppError::new)?;
    let plan = apply_output_preset(plan, config.output_preset)?;
    let plan = if args.placement() == PlacementArg::InPlace {
        plan
    } else {
        organization_plan(
            &args,
            &output_root,
            OrganizationMedia::Movie(&resolved),
            None,
            plan,
        )?
    };
    finish_plan(
        plan,
        &args,
        &output_root,
        PlanDiagnostics::new(&result.warnings, &resolved.warnings),
        mode,
        FinalizationPolicy::new(config.conflict_policy, conflicts),
    )
}

async fn scrape_music(args: ScrapeArgs, config: &Config, mode: OutputMode) -> AppResult<RunStatus> {
    if args.placement() != PlacementArg::InPlace {
        return Err(AppError::invalid_input(
            "music scrape currently supports only in-place placement",
        ));
    }
    let scan_root = scan_root(&args.path)?;
    let result = scan_music(scan_root).map_err(AppError::new)?;
    if result.documents.is_empty() {
        return Err(AppError::invalid_input("no local music metadata was found"));
    }
    if result.documents.len() != 1 {
        return Err(AppError::invalid_input(format!(
            "ambiguous music input: found {} albums; scrape one album at a time",
            result.documents.len()
        )));
    }
    let title = result.documents[0]
        .titles
        .entries()
        .first()
        .map(|entry| entry.value().clone())
        .ok_or_else(|| AppError::invalid_input("local music album has no title"))?;
    let output_root = result.roots[0].clone();
    let warnings = result.warnings;
    let provider = LocalProvider::from_music_documents(result.documents).map_err(AppError::new)?;
    let fixer = super::build_fixer(provider, config)?;
    let resolved = fixer.music(title).resolve().await.map_err(AppError::new)?;
    let conflicts = resolved.conflicts.len();
    let plan = MusicWriter::default()
        .plan_resolved(&resolved, &output_root)
        .map_err(AppError::new)?;
    let plan = apply_output_preset(plan, config.output_preset)?;
    finish_plan(
        plan,
        &args,
        &output_root,
        PlanDiagnostics::new(&warnings, &resolved.warnings),
        mode,
        FinalizationPolicy::new(config.conflict_policy, conflicts),
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
        return Err(AppError::invalid_input(
            "no local television episodes were found",
        ));
    }
    if result.documents.len() != 1 {
        return Err(AppError::invalid_input(format!(
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
        .ok_or_else(|| AppError::invalid_input("local television series has no title"))?;
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
    let output_root = if args.placement() == PlacementArg::InPlace {
        series_root.clone()
    } else {
        series_root.parent().unwrap_or(series_root).to_path_buf()
    };
    let conflicts = resolved.conflicts.len();
    let plan = TelevisionWriter
        .plan_resolved(&resolved, &output_root)
        .map_err(AppError::new)?;
    let plan = apply_output_preset(plan, config.output_preset)?;
    let plan = if args.placement() == PlacementArg::InPlace {
        plan
    } else {
        organization_plan(
            &args,
            &output_root,
            OrganizationMedia::Television(&resolved),
            placement_target.as_deref(),
            plan,
        )?
    };
    finish_plan(
        plan,
        &args,
        &output_root,
        PlanDiagnostics::new(&warnings, &resolved.warnings),
        mode,
        FinalizationPolicy::new(config.conflict_policy, conflicts),
    )
}

fn finish_plan(
    plan: fixer_core::OutputPlan,
    args: &ScrapeArgs,
    output_root: &Path,
    diagnostics: PlanDiagnostics<'_>,
    mode: OutputMode,
    policy: FinalizationPolicy,
) -> AppResult<RunStatus> {
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
            print_plan_text(&plan, output_root);
        }
        if policy.requires_review() {
            eprintln!("review required: {} metadata conflict(s)", policy.conflicts);
            return Ok(RunStatus::ReviewRequired);
        }
        return Ok(super::finish_with_resolution_warnings(
            diagnostics.scan,
            diagnostics.resolution,
        ));
    }

    if policy.requires_review() {
        print_plan_text(&plan, output_root);
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
    Ok(super::finish_with_resolution_warnings(
        diagnostics.scan,
        diagnostics.resolution,
    ))
}

fn print_plan_text(plan: &fixer_core::OutputPlan, output_root: &Path) {
    println!(
        "planned {} operation(s) at {}",
        plan.operations().len(),
        output_root.display()
    );
    for operation in plan.operations() {
        let name = match operation {
            fixer_core::OutputOperation::CreateDirectory { .. } => "create_directory",
            fixer_core::OutputOperation::WriteBytes { .. } => "write_bytes",
            fixer_core::OutputOperation::Copy { .. } => "copy",
            fixer_core::OutputOperation::Move { .. } => "move",
            fixer_core::OutputOperation::Symlink { .. } => "symlink",
            fixer_core::OutputOperation::Hardlink { .. } => "hardlink",
            fixer_core::OutputOperation::Reflink { .. } => "reflink",
        };
        if let Some(source) = operation.source() {
            println!(
                "{name} {} -> {}",
                source.display(),
                operation
                    .target()
                    .expect("output operations have targets")
                    .display()
            );
        } else {
            println!(
                "{name} {}",
                operation
                    .target()
                    .expect("output operations have targets")
                    .display()
            );
        }
    }
}

fn scan_root(path: &Path) -> AppResult<&Path> {
    if path.is_dir() {
        Ok(path)
    } else {
        path.parent()
            .ok_or_else(|| AppError::invalid_input("input path has no parent directory"))
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
        .ok_or_else(|| AppError::invalid_input("no local movie document was found"))?;
    let title = movie
        .titles
        .entries()
        .first()
        .map(|entry| entry.value().clone())
        .ok_or_else(|| AppError::invalid_input("local movie has no title"))?;
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
        .ok_or_else(|| AppError::invalid_input("media path has no file name"))?;
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

fn organization_plan(
    args: &ScrapeArgs,
    destination_path: &Path,
    media: OrganizationMedia<'_>,
    media_target: Option<&Path>,
    metadata_plan: fixer_core::OutputPlan,
) -> AppResult<fixer_core::OutputPlan> {
    if !args.path.is_file() {
        return Err(AppError::invalid_input(
            "non-in-place placement requires a media file path",
        ));
    }
    let source = args.path.canonicalize().map_err(AppError::new)?;
    let mut request = OrganizationRequest::new(
        &source,
        destination_path,
        media,
        organization_placement(args.placement()),
        metadata_plan,
    );
    if let Some(target) = media_target {
        request = request.with_media_target(target);
    }
    organize(request).map_err(AppError::new)
}

fn organization_placement(placement: PlacementArg) -> OrganizationPlacement {
    match placement {
        PlacementArg::InPlace => unreachable!("in-place plans bypass organization"),
        PlacementArg::Move => OrganizationPlacement::Move,
        PlacementArg::Symlink => OrganizationPlacement::Symlink,
        PlacementArg::Hardlink => OrganizationPlacement::Hardlink,
        PlacementArg::Copy => OrganizationPlacement::Copy,
        PlacementArg::Reflink => OrganizationPlacement::Reflink,
    }
}

fn apply_output_preset(
    plan: fixer_core::OutputPlan,
    preset: OutputPreset,
) -> AppResult<fixer_core::OutputPlan> {
    if preset == OutputPreset::Full {
        Ok(plan)
    } else {
        metadata_only(&plan).map_err(AppError::new)
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
