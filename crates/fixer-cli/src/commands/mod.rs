pub mod resolve;
mod scan;
pub mod scrape;
pub mod search;

use crate::{
    AppError, AppResult, RunStatus,
    args::{Command, ConfigCommand, ProvidersCommand},
    config::Config,
};
use fixer_core::ExternalId;
use fixer_provider_local::{LocalProvider, ScanWarning};
use fixer_sdk::Fixer;

pub async fn run(command: Command, config: Config) -> AppResult<RunStatus> {
    match command {
        Command::Search { command } => search::run(command, &config).await,
        Command::Resolve { command } => resolve::run(command, &config).await,
        Command::Scan(args) => scan::run(&args),
        Command::Plan(args) => scrape::plan(args, &config).await,
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
            println!("local\tmovie,television,anime,music,book\toffline");
            println!("tmdb\tmovie,television\tnetwork");
            println!("bangumi\tanime\tnetwork");
            println!("anilist\tanime\tnetwork,optional");
            println!("musicbrainz\tmusic\tnetwork");
            println!("openlibrary\tbook\tnetwork");
            Ok(RunStatus::Success)
        }
    }
}

pub fn local_fixer(config: &Config) -> AppResult<(Fixer, Vec<ScanWarning>)> {
    let root = config.local_root.as_ref().ok_or_else(|| {
        AppError::invalid_input(
            "local metadata root is required; pass --local-root or set FIXER_LOCAL_ROOT",
        )
    })?;
    let (provider, warnings) = LocalProvider::from_scan(root).map_err(AppError::new)?;
    Ok((build_fixer(provider, config)?, warnings))
}
pub fn build_fixer(provider: LocalProvider, config: &Config) -> AppResult<Fixer> {
    fixer_runtime::build_fixer(config.shared(), provider).map_err(AppError::new)
}
pub fn parse_external_ids(values: &[String]) -> AppResult<Vec<ExternalId>> {
    values
        .iter()
        .map(|value| {
            let (namespace, id) = value.split_once(':').ok_or_else(|| {
                AppError::invalid_input(format!(
                    "external ID `{value}` must use namespace:id syntax"
                ))
            })?;
            ExternalId::new(namespace, id).map_err(AppError::invalid_input)
        })
        .collect()
}

pub fn finish_with_warnings(warnings: &[ScanWarning]) -> RunStatus {
    finish_with_resolution_warnings(warnings, &[])
}

pub fn finish_with_resolution_warnings(
    scan_warnings: &[ScanWarning],
    resolution_warnings: &[fixer_core::ResolutionWarning],
) -> RunStatus {
    for warning in scan_warnings {
        eprintln!("warning: {}: {}", warning.path.display(), warning.message);
    }
    for warning in resolution_warnings {
        eprintln!("warning: {}: {}", warning.code, warning.message);
    }
    if scan_warnings.is_empty() && resolution_warnings.is_empty() {
        RunStatus::Success
    } else {
        RunStatus::PartialSuccess
    }
}
