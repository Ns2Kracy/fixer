use crate::{AppError, AppResult};
use serde::Deserialize;
use std::{
    env, fmt, fs,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigSource {
    Flag,
    Environment,
    File,
    Default,
}
impl fmt::Display for ConfigSource {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Flag => "flag",
            Self::Environment => "environment",
            Self::File => "file",
            Self::Default => "default",
        })
    }
}

#[derive(Clone)]
pub struct Config {
    pub offline: bool,
    pub proxy: Option<String>,
    pub local_root: Option<PathBuf>,
    api_key: Option<String>,
    tmdb_base_url: Option<String>,
    bangumi_base_url: Option<String>,
    musicbrainz_base_url: Option<String>,
    anilist_enabled: bool,
    anilist_endpoint: Option<String>,
    anilist_access_token: Option<String>,
    anilist_source: ConfigSource,
    offline_source: ConfigSource,
    proxy_source: ConfigSource,
    local_root_source: ConfigSource,
    api_key_source: ConfigSource,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileConfig {
    offline: Option<bool>,
    proxy: Option<String>,
    local_root: Option<PathBuf>,
    #[serde(alias = "tmdb_api_token")]
    api_key: Option<String>,
    tmdb_base_url: Option<String>,
    bangumi_base_url: Option<String>,
    musicbrainz_base_url: Option<String>,
    anilist_enabled: Option<bool>,
    anilist_endpoint: Option<String>,
    anilist_access_token: Option<String>,
}

impl Config {
    pub fn load(cli: &crate::args::Cli) -> AppResult<Self> {
        let config_path = cli
            .config
            .clone()
            .or_else(|| env::var_os("FIXER_CONFIG").map(PathBuf::from));
        let file = match config_path {
            Some(path) => read_file(&path)?,
            None if Path::new("fixer.json").is_file() => read_file(Path::new("fixer.json"))?,
            None => FileConfig::default(),
        };
        let env_offline = env::var("FIXER_OFFLINE")
            .ok()
            .map(|value| parse_bool("FIXER_OFFLINE", &value))
            .transpose()?;
        let (offline, offline_source) = if cli.offline {
            (true, ConfigSource::Flag)
        } else if let Some(value) = env_offline {
            (value, ConfigSource::Environment)
        } else if let Some(value) = file.offline {
            (value, ConfigSource::File)
        } else {
            (false, ConfigSource::Default)
        };
        let (proxy, proxy_source) =
            pick(cli.proxy.clone(), env::var("FIXER_PROXY").ok(), file.proxy);
        let (local_root, local_root_source) = pick(
            cli.local_root.clone(),
            env::var_os("FIXER_LOCAL_ROOT").map(PathBuf::from),
            file.local_root,
        );
        let (api_key, api_key_source) = pick(
            None::<String>,
            env::var("TMDB_API_TOKEN")
                .ok()
                .or_else(|| env::var("FIXER_API_KEY").ok()),
            file.api_key,
        );
        let tmdb_base_url = env::var("TMDB_BASE_URL").ok().or(file.tmdb_base_url);
        let bangumi_base_url = env::var("BANGUMI_BASE_URL").ok().or(file.bangumi_base_url);
        let musicbrainz_base_url = env::var("MUSICBRAINZ_BASE_URL")
            .ok()
            .or(file.musicbrainz_base_url);
        let env_anilist_enabled = env::var("FIXER_ANILIST_ENABLED")
            .ok()
            .map(|value| parse_bool("FIXER_ANILIST_ENABLED", &value))
            .transpose()?;
        let (anilist_enabled, anilist_source) = if let Some(enabled) = env_anilist_enabled {
            (enabled, ConfigSource::Environment)
        } else if let Some(enabled) = file.anilist_enabled {
            (enabled, ConfigSource::File)
        } else {
            (false, ConfigSource::Default)
        };
        let anilist_endpoint = env::var("ANILIST_ENDPOINT").ok().or(file.anilist_endpoint);
        let anilist_access_token = env::var("ANILIST_ACCESS_TOKEN")
            .ok()
            .or(file.anilist_access_token);
        Ok(Self {
            offline,
            proxy,
            local_root,
            api_key,
            tmdb_base_url,
            bangumi_base_url,
            musicbrainz_base_url,
            anilist_enabled,
            anilist_endpoint,
            anilist_access_token,
            anilist_source,
            offline_source,
            proxy_source,
            local_root_source,
            api_key_source,
        })
    }

    pub fn tmdb_provider(&self) -> AppResult<Option<fixer_provider_tmdb::TmdbProvider>> {
        let Some(token) = &self.api_key else {
            return Ok(None);
        };
        let mut config =
            fixer_provider_tmdb::TmdbConfig::new(token.clone()).map_err(AppError::new)?;
        if let Some(base_url) = &self.tmdb_base_url {
            config = config.with_base_url(base_url).map_err(AppError::new)?;
        }
        fixer_provider_tmdb::TmdbProvider::new(config)
            .map(Some)
            .map_err(AppError::new)
    }

    pub fn bangumi_provider(&self) -> AppResult<fixer_provider_bangumi::BangumiProvider> {
        let mut config = fixer_provider_bangumi::BangumiConfig::default();
        if let Some(base_url) = &self.bangumi_base_url {
            config = config.with_base_url(base_url).map_err(AppError::new)?;
        }
        fixer_provider_bangumi::BangumiProvider::new(config).map_err(AppError::new)
    }

    pub fn musicbrainz_provider(
        &self,
    ) -> AppResult<fixer_provider_musicbrainz::MusicBrainzProvider> {
        let mut config = fixer_provider_musicbrainz::MusicBrainzConfig::default();
        if let Some(base_url) = &self.musicbrainz_base_url {
            config = config.with_base_url(base_url).map_err(AppError::new)?;
        }
        fixer_provider_musicbrainz::MusicBrainzProvider::new(config).map_err(AppError::new)
    }

    pub fn anilist_provider(&self) -> AppResult<Option<fixer_provider_anilist::AniListProvider>> {
        if !self.anilist_enabled {
            return Ok(None);
        }
        let mut config = fixer_provider_anilist::AniListConfig::default();
        if let Some(endpoint) = &self.anilist_endpoint {
            config = config.with_endpoint(endpoint).map_err(AppError::new)?;
        }
        if let Some(access_token) = &self.anilist_access_token {
            config = config
                .with_access_token(access_token.clone())
                .map_err(AppError::new)?;
        }
        fixer_provider_anilist::AniListProvider::new(config)
            .map(Some)
            .map_err(AppError::new)
    }

    pub fn validation_summary(&self) -> String {
        format!(
            "configuration valid\noffline: {} ({})\nproxy: {} ({})\nlocal_root: {} ({})\napi_key: {} ({})\nanilist: {} ({})\n",
            self.offline,
            self.offline_source,
            configured(&self.proxy),
            self.proxy_source,
            configured(&self.local_root),
            self.local_root_source,
            configured(&self.api_key),
            self.api_key_source,
            if self.anilist_enabled {
                "enabled"
            } else {
                "disabled"
            },
            self.anilist_source,
        )
    }
}

impl fmt::Debug for Config {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Config")
            .field("offline", &self.offline)
            .field("proxy", &self.proxy)
            .field("local_root", &self.local_root)
            .field("api_key", &self.api_key.as_ref().map(|_| "[REDACTED]"))
            .field("tmdb_base_url", &self.tmdb_base_url)
            .field("bangumi_base_url", &self.bangumi_base_url)
            .field("musicbrainz_base_url", &self.musicbrainz_base_url)
            .field("anilist_enabled", &self.anilist_enabled)
            .field(
                "anilist_endpoint",
                &self.anilist_endpoint.as_ref().map(|_| "[CONFIGURED]"),
            )
            .field(
                "anilist_access_token",
                &self.anilist_access_token.as_ref().map(|_| "[REDACTED]"),
            )
            .finish()
    }
}

fn read_file(path: &Path) -> AppResult<FileConfig> {
    let input = fs::read_to_string(path).map_err(AppError::new)?;
    serde_json::from_str(&input).map_err(AppError::new)
}
fn parse_bool(name: &str, value: &str) -> AppResult<bool> {
    match value.to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Ok(true),
        "0" | "false" | "no" | "off" => Ok(false),
        _ => Err(AppError::new(format!("{name} must be a boolean"))),
    }
}
fn pick<T>(flag: Option<T>, environment: Option<T>, file: Option<T>) -> (Option<T>, ConfigSource) {
    if let Some(value) = flag {
        (Some(value), ConfigSource::Flag)
    } else if let Some(value) = environment {
        (Some(value), ConfigSource::Environment)
    } else if let Some(value) = file {
        (Some(value), ConfigSource::File)
    } else {
        (None, ConfigSource::Default)
    }
}
fn configured<T>(value: &Option<T>) -> &'static str {
    if value.is_some() {
        "configured"
    } else {
        "not configured"
    }
}
