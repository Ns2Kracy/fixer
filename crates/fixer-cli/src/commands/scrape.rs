use crate::{
    AppError, AppResult, RunStatus,
    args::{MediaKindArg, PlacementArg, ScrapeArgs},
    config::Config,
};
use fixer_core::{LocalizedValue, Movie, MovieRelease, ReleaseDate, ReleaseId, WorkId};
use fixer_provider_local::{
    EpisodeHint, LocalProvider, MediaHint, ScanWarning, identify_episode_path, identify_path,
    parse_matroska_tags, scan, scan_television,
};
use fixer_sdk::output::{ExecutionPolicy, OutputPlanExt, PlacementMode, plan_media_placement};
use fixer_writer_local::{JsonWriter, PathTemplate, TelevisionWriter, TemplateContext};
use std::path::{Path, PathBuf};

pub async fn run(args: ScrapeArgs, config: &Config) -> AppResult<RunStatus> {
    if !args.path.exists() {
        return Err(AppError::new(format!(
            "input path does not exist: {}",
            args.path.display()
        )));
    }
    match args.kind {
        MediaKindArg::Movie => scrape_movie(args, config).await,
        MediaKindArg::Television => scrape_television(args, config).await,
    }
}

async fn scrape_movie(args: ScrapeArgs, config: &Config) -> AppResult<RunStatus> {
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
    let output_root = movie_output_root(&args.path, args.placement, &resolved)?;
    let plan = JsonWriter
        .plan_resolved(&resolved, &output_root)
        .map_err(AppError::new)?;
    execute_plan(plan, &args, &output_root, &result.warnings, None)
}

async fn scrape_television(args: ScrapeArgs, config: &Config) -> AppResult<RunStatus> {
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
    let placement_target = (args.placement != PlacementArg::InPlace)
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
    let output_root = television_output_root(series_root, args.placement, &resolved)?;
    let plan = TelevisionWriter
        .plan_resolved(&resolved, &output_root)
        .map_err(AppError::new)?;
    execute_plan(
        plan,
        &args,
        &output_root,
        &warnings,
        placement_target.as_deref(),
    )
}

fn execute_plan(
    mut plan: fixer_core::OutputPlan,
    args: &ScrapeArgs,
    output_root: &Path,
    warnings: &[ScanWarning],
    placement_target: Option<&Path>,
) -> AppResult<RunStatus> {
    if args.placement != PlacementArg::InPlace {
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
                placement_mode(args.placement),
            )
            .map_err(AppError::new)?;
            for operation in placement.operations() {
                plan.push(operation.clone());
            }
        }
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
