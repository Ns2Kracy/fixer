use crate::{
    AppError, AppResult, RunStatus,
    args::{ResolveAnimeArgs, ResolveCommand, ResolveMovieArgs, ResolveTelevisionArgs},
    config::Config,
    render::{self, ResolvedAnimeDto, ResolvedMovieDto, ResolvedTelevisionDto},
};

pub async fn run(command: ResolveCommand, config: &Config) -> AppResult<RunStatus> {
    match command {
        ResolveCommand::Anime(args) => resolve_anime(args, config).await,
        ResolveCommand::Movie(args) => resolve_movie(args, config).await,
        ResolveCommand::Television(args) => resolve_television(args, config).await,
    }
}

async fn resolve_anime(args: ResolveAnimeArgs, config: &Config) -> AppResult<RunStatus> {
    let (fixer, warnings) = super::local_fixer(config)?;
    let external_ids = super::parse_external_ids(&args.query.external_ids)?;
    let mut query = fixer.anime(args.query.title);
    if let Some(year) = args.query.year {
        query = query.year(year);
    }
    for external_id in external_ids {
        query = query.external_id(external_id);
    }
    let resolved = query.resolve().await.map_err(AppError::new)?;
    if args.json {
        render::json(&ResolvedAnimeDto::from_resolved(&resolved))?;
    } else {
        render::resolved_anime_text(&resolved);
    }
    Ok(super::finish_with_warnings(&warnings))
}

async fn resolve_movie(args: ResolveMovieArgs, config: &Config) -> AppResult<RunStatus> {
    let (fixer, warnings) = super::local_fixer(config)?;
    let mut query = fixer.movie(args.query.title);
    if let Some(year) = args.query.year {
        query = query.year(year);
    }
    let resolved = query.resolve().await.map_err(AppError::new)?;
    if args.json {
        render::json(&ResolvedMovieDto::from_resolved(&resolved))?;
    } else {
        render::resolved_movie_text(&resolved);
    }
    Ok(super::finish_with_warnings(&warnings))
}

async fn resolve_television(args: ResolveTelevisionArgs, config: &Config) -> AppResult<RunStatus> {
    let (fixer, warnings) = super::local_fixer(config)?;
    let external_ids = super::parse_external_ids(&args.query.external_ids)?;
    let mut query = fixer.television(args.query.title);
    if let Some(year) = args.query.year {
        query = query.year(year);
    }
    if let Some(ordering) = args.query.ordering {
        query = query.ordering(ordering.into());
    }
    for external_id in external_ids {
        query = query.external_id(external_id);
    }
    let resolved = query.resolve().await.map_err(AppError::new)?;
    if args.json {
        render::json(&ResolvedTelevisionDto::from_resolved(&resolved))?;
    } else {
        render::resolved_television_text(&resolved);
    }
    Ok(super::finish_with_warnings(&warnings))
}
