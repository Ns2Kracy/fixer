use crate::{
    AppError, AppResult, RunStatus,
    args::{MediaKindArg, PlacementArg, ScrapeArgs},
    config::Config,
};
use fixer_core::{LocalizedValue, Movie, MovieRelease, ReleaseDate, ReleaseId, WorkId};
use fixer_provider_local::{LocalProvider, MediaHint, identify_path, scan};
use fixer_sdk::output::{ExecutionPolicy, OutputPlanExt, PlacementMode, plan_media_placement};
use fixer_writer_local::{JsonWriter, PathTemplate, TemplateContext};
use std::path::{Path, PathBuf};

pub async fn run(args: ScrapeArgs, config: &Config) -> AppResult<RunStatus> {
    let MediaKindArg::Movie = args.kind;
    if !args.path.exists() {
        return Err(AppError::new(format!(
            "input path does not exist: {}",
            args.path.display()
        )));
    }
    let scan_root = if args.path.is_dir() {
        args.path.as_path()
    } else {
        args.path
            .parent()
            .ok_or_else(|| AppError::new("input path has no parent directory"))?
    };
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
    let (query_title, query_year) = query_for(&args.path, hint.as_ref(), &result.documents)?;
    let provider = LocalProvider::from_documents(result.documents).map_err(AppError::new)?;
    let fixer = super::build_fixer(provider, config)?;
    let mut query = fixer.movie(query_title);
    if let Some(year) = query_year {
        query = query.year(year);
    }
    let resolved = query.resolve().await.map_err(AppError::new)?;
    let output_root = output_root(&args.path, args.placement, &resolved)?;
    let mut plan = JsonWriter
        .plan_resolved(&resolved, &output_root)
        .map_err(AppError::new)?;
    if args.placement != PlacementArg::InPlace {
        if !args.path.is_file() {
            return Err(AppError::new(
                "non-in-place placement requires a media file path",
            ));
        }
        let target = args
            .path
            .file_name()
            .ok_or_else(|| AppError::new("media path has no file name"))?;
        let placement = plan_media_placement(
            &args.path,
            &output_root,
            PathBuf::from(target),
            placement_mode(args.placement),
        )
        .map_err(AppError::new)?;
        for operation in placement.operations() {
            plan.push(operation.clone());
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
    Ok(super::finish_with_warnings(&result.warnings))
}

fn query_for(
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

fn output_root(
    path: &Path,
    placement: PlacementArg,
    resolved: &fixer_core::Resolved<Movie>,
) -> AppResult<PathBuf> {
    let base = if path.is_dir() {
        path
    } else {
        path.parent()
            .ok_or_else(|| AppError::new("input path has no parent directory"))?
    };
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

const fn placement_mode(placement: PlacementArg) -> PlacementMode {
    match placement {
        PlacementArg::InPlace => PlacementMode::InPlace,
        PlacementArg::Symlink => PlacementMode::RelativeSymlink,
        PlacementArg::Hardlink => PlacementMode::Hardlink,
        PlacementArg::Copy => PlacementMode::Copy,
        PlacementArg::Reflink => PlacementMode::Reflink,
    }
}
