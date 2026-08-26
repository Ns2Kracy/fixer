use crate::{
    AppError, AppResult, RunStatus,
    args::{MediaKindArg, PlacementArg, PlanArgs, ScrapeArgs},
    config::{Config, ConflictPolicy},
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
struct ConflictOutcome {
    policy: ConflictPolicy,
    count: usize,
}

impl ConflictOutcome {
    const fn new(policy: ConflictPolicy, count: usize) -> Self {
        Self { policy, count }
    }

    const fn requires_review(self) -> bool {
        self.count > 0 && matches!(self.policy, ConflictPolicy::Review)
    }

    const fn rejects(self) -> bool {
        self.count > 0 && matches!(self.policy, ConflictPolicy::Error)
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
        ConflictOutcome::new(config.conflict_policy, conflicts),
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
        ConflictOutcome::new(config.conflict_policy, conflicts),
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
        ConflictOutcome::new(config.conflict_policy, conflicts),
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
        ConflictOutcome::new(config.conflict_policy, conflicts),
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
        ConflictOutcome::new(config.conflict_policy, conflicts),
    )
}

fn finish_plan(
    mut plan: fixer_core::OutputPlan,
    args: &ScrapeArgs,
    output_root: &Path,
    warnings: &[ScanWarning],
    placement_target: Option<&Path>,
    mode: OutputMode,
    conflicts: ConflictOutcome,
) -> AppResult<RunStatus> {
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
    if conflicts.rejects() {
        return Err(AppError::new(format!(
            "conflict policy rejected {} metadata conflict(s)",
            conflicts.count
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
        if conflicts.requires_review() {
            eprintln!("review required: {} metadata conflict(s)", conflicts.count);
            return Ok(RunStatus::ReviewRequired);
        }
        return Ok(super::finish_with_warnings(warnings));
    }

    if conflicts.requires_review() {
        eprintln!("review required: {} metadata conflict(s)", conflicts.count);
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

const fn placement_mode(placement: PlacementArg) -> PlacementMode {
    match placement {
        PlacementArg::InPlace => PlacementMode::InPlace,
        PlacementArg::Symlink => PlacementMode::RelativeSymlink,
        PlacementArg::Hardlink => PlacementMode::Hardlink,
        PlacementArg::Copy => PlacementMode::Copy,
        PlacementArg::Reflink => PlacementMode::Reflink,
    }
}
