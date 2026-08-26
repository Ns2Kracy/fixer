use crate::{
    AppError, AppResult, RunStatus,
    args::{MovieQueryArgs, SearchCommand, TelevisionQueryArgs},
    config::Config,
    render,
};

pub async fn run(command: SearchCommand, config: &Config) -> AppResult<RunStatus> {
    match command {
        SearchCommand::Movie(args) => search_movie(args, config).await,
        SearchCommand::Television(args) => search_television(args, config).await,
    }
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

async fn search_television(args: TelevisionQueryArgs, config: &Config) -> AppResult<RunStatus> {
    let (fixer, warnings) = super::local_fixer(config)?;
    let external_ids = super::parse_external_ids(&args.external_ids)?;
    let mut query = fixer.television(args.title);
    if let Some(year) = args.year {
        query = query.year(year);
    }
    if let Some(ordering) = args.ordering {
        query = query.ordering(ordering.into());
    }
    for external_id in external_ids {
        query = query.external_id(external_id);
    }
    let results = query.search().await.map_err(AppError::new)?;
    render::search_text(results.candidates());
    Ok(super::finish_with_warnings(&warnings))
}
