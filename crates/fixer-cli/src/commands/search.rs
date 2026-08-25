use crate::{
    AppError, AppResult, RunStatus,
    args::{MovieQueryArgs, SearchCommand},
    config::Config,
    render,
};

pub async fn run(command: SearchCommand, config: &Config) -> AppResult<RunStatus> {
    let SearchCommand::Movie(args) = command;
    search_movie(args, config).await
}
async fn search_movie(args: MovieQueryArgs, config: &Config) -> AppResult<RunStatus> {
    let (fixer, warnings) = super::local_fixer(config)?;
    let mut query = fixer.movie(args.title);
    if let Some(year) = args.year {
        query = query.year(year);
    }
    let results = query.search().await.map_err(AppError::new)?;
    render::search_text(results.candidates());
    Ok(super::finish_with_warnings(&warnings))
}
