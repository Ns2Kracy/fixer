pub mod resolve;
pub mod scrape;
pub mod search;

use crate::{
    AppError, AppResult, RunStatus,
    args::{Command, ConfigCommand, ProvidersCommand},
    config::Config,
};
use fixer_provider_local::{LocalProvider, ScanWarning};
use fixer_sdk::Fixer;

pub async fn run(command: Command, config: Config) -> AppResult<RunStatus> {
    match command {
        Command::Search { command } => search::run(command, &config).await,
        Command::Resolve { command } => resolve::run(command, &config).await,
        Command::Scrape(args) => scrape::run(args, &config).await,
        Command::Config {
            command: ConfigCommand::Validate,
        } => {
            print!("{}", config.validation_summary());
            Ok(RunStatus::Success)
        }
        Command::Providers {
            command: ProvidersCommand::List,
        } => {
            println!("local\tmovie\toffline");
            Ok(RunStatus::Success)
        }
    }
}

pub(crate) fn local_fixer(config: &Config) -> AppResult<(Fixer, Vec<ScanWarning>)> {
    let root = config.local_root.as_ref().ok_or_else(|| {
        AppError::new("local metadata root is required; pass --local-root or set FIXER_LOCAL_ROOT")
    })?;
    let (provider, warnings) = LocalProvider::from_scan(root).map_err(AppError::new)?;
    Ok((build_fixer(provider, config)?, warnings))
}
pub(crate) fn build_fixer(provider: LocalProvider, config: &Config) -> AppResult<Fixer> {
    let mut builder = Fixer::builder()
        .provider(provider)
        .preferred_languages(["zh-CN", "en", "und"])
        .map_err(AppError::new)?;
    if let Some(tmdb) = config.tmdb_provider()? {
        builder = builder.provider(tmdb);
    }
    if let Some(proxy) = &config.proxy {
        builder = builder.proxy(proxy.clone());
    }
    if config.offline {
        builder = builder.offline();
    }
    builder.build().map_err(AppError::new)
}
pub(crate) fn finish_with_warnings(warnings: &[ScanWarning]) -> RunStatus {
    for warning in warnings {
        eprintln!("warning: {}: {}", warning.path.display(), warning.message);
    }
    if warnings.is_empty() {
        RunStatus::Success
    } else {
        RunStatus::PartialSuccess
    }
}
