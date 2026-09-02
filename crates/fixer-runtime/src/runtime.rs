use std::time::Duration;

use fixer_provider_local::LocalProvider;
use fixer_sdk::{Fixer, FixerBuilder};
use thiserror::Error;

use crate::{ConfigLoadError, FixerConfig};

#[derive(Debug, Error)]
pub enum RuntimeConfigError {
    #[error(transparent)]
    Configuration(#[from] ConfigLoadError),
    #[error("invalid {provider} provider configuration: {message}")]
    Provider {
        provider: &'static str,
        message: String,
    },
    #[error(transparent)]
    Sdk(#[from] fixer_sdk::SdkError),
}

pub fn build_fixer(
    config: &FixerConfig,
    local_provider: LocalProvider,
) -> Result<Fixer, RuntimeConfigError> {
    config.validate()?;
    let mut builder = Fixer::builder()
        .preferred_languages(config.preferred_locales.iter())?
        .timeout(Duration::from_secs(config.timeout_seconds));

    if config.provider_enabled("local") {
        builder = builder.provider(local_provider);
    }
    builder = add_tmdb(builder, config)?;
    builder = add_bangumi(builder, config)?;
    builder = add_musicbrainz(builder, config)?;
    builder = add_openlibrary(builder, config)?;
    builder = add_anilist(builder, config)?;

    if let Some(proxy) = &config.proxy {
        builder = builder.proxy(proxy.clone());
    }
    if config.offline {
        builder = builder.offline();
    }
    Ok(builder.build()?)
}

fn add_tmdb(
    builder: FixerBuilder,
    config: &FixerConfig,
) -> Result<FixerBuilder, RuntimeConfigError> {
    if !config.provider_enabled("tmdb") {
        return Ok(builder);
    }
    let Some(token) = config.providers.tmdb.resolved_api_token() else {
        return Ok(builder);
    };
    let provider_config = fixer_provider_tmdb::TmdbConfig::new(token.to_owned())
        .and_then(|provider_config| provider_config.with_base_url(&config.providers.tmdb.base_url))
        .map_err(|error| provider_error("tmdb", error))?;
    let provider = fixer_provider_tmdb::TmdbProvider::new(provider_config)
        .map_err(|error| provider_error("tmdb", error))?;
    Ok(builder.provider(provider))
}

fn add_bangumi(
    builder: FixerBuilder,
    config: &FixerConfig,
) -> Result<FixerBuilder, RuntimeConfigError> {
    if !config.provider_enabled("bangumi") {
        return Ok(builder);
    }
    let provider_config = fixer_provider_bangumi::BangumiConfig::default()
        .with_base_url(&config.providers.bangumi.base_url)
        .map_err(|error| provider_error("bangumi", error))?;
    let provider = fixer_provider_bangumi::BangumiProvider::new(provider_config)
        .map_err(|error| provider_error("bangumi", error))?;
    Ok(builder.provider(provider))
}

fn add_musicbrainz(
    builder: FixerBuilder,
    config: &FixerConfig,
) -> Result<FixerBuilder, RuntimeConfigError> {
    if !config.provider_enabled("musicbrainz") {
        return Ok(builder);
    }
    let provider_config = fixer_provider_musicbrainz::MusicBrainzConfig::default()
        .with_base_url(&config.providers.musicbrainz.base_url)
        .map_err(|error| provider_error("musicbrainz", error))?;
    let provider = fixer_provider_musicbrainz::MusicBrainzProvider::new(provider_config)
        .map_err(|error| provider_error("musicbrainz", error))?;
    Ok(builder.provider(provider))
}

fn add_openlibrary(
    builder: FixerBuilder,
    config: &FixerConfig,
) -> Result<FixerBuilder, RuntimeConfigError> {
    if !config.provider_enabled("openlibrary") {
        return Ok(builder);
    }
    let provider_config = fixer_provider_openlibrary::OpenLibraryConfig::default()
        .with_api_base_url(&config.providers.openlibrary.base_url)
        .and_then(|provider_config| {
            provider_config.with_cover_base_url(&config.providers.openlibrary.cover_base_url)
        })
        .map_err(|error| provider_error("openlibrary", error))?;
    let provider = fixer_provider_openlibrary::OpenLibraryProvider::new(provider_config)
        .map_err(|error| provider_error("openlibrary", error))?;
    Ok(builder.provider(provider))
}

fn add_anilist(
    builder: FixerBuilder,
    config: &FixerConfig,
) -> Result<FixerBuilder, RuntimeConfigError> {
    if !config.provider_enabled("anilist") {
        return Ok(builder);
    }
    let mut provider_config = fixer_provider_anilist::AniListConfig::default()
        .with_endpoint(&config.providers.anilist.base_url)
        .map_err(|error| provider_error("anilist", error))?;
    if let Some(token) = config.providers.anilist.resolved_access_token() {
        provider_config = provider_config
            .with_access_token(token.to_owned())
            .map_err(|error| provider_error("anilist", error))?;
    }
    let provider = fixer_provider_anilist::AniListProvider::new(provider_config)
        .map_err(|error| provider_error("anilist", error))?;
    Ok(builder.provider(provider))
}

fn provider_error(provider: &'static str, error: impl std::fmt::Display) -> RuntimeConfigError {
    RuntimeConfigError::Provider {
        provider,
        message: error.to_string(),
    }
}
