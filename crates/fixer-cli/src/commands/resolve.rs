use crate::{
    AppError, AppResult, RunStatus,
    args::{ResolveCommand, ResolveMovieArgs},
    config::Config,
    render::{self, ResolvedMovieDto},
};

pub async fn run(command: ResolveCommand, config: &Config) -> AppResult<RunStatus> {
    let ResolveCommand::Movie(args) = command;
    resolve_movie(args, config).await
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
        render::resolved_text(&resolved);
    }
    Ok(super::finish_with_warnings(&warnings))
}
