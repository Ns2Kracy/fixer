use std::{env, fmt, ops::Deref};

use fixer_runtime::{ConfigLoader, ConfigOverrides, FixerConfig, LoadedConfig};

use crate::{AppError, AppResult};

pub use fixer_runtime::{ConflictPolicy, OutputPreset, PlacementPolicy};

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

#[derive(Debug, Clone, Copy)]
struct ConfigSources {
    offline: ConfigSource,
    proxy: ConfigSource,
    local_root: ConfigSource,
    api_key: ConfigSource,
    anilist: ConfigSource,
}

#[derive(Clone)]
pub struct Config {
    shared: FixerConfig,
    sources: ConfigSources,
}

impl Config {
    pub fn load(cli: &crate::args::Cli) -> AppResult<Self> {
        let cwd = env::current_dir().map_err(AppError::new)?;
        let overrides = ConfigOverrides {
            offline: cli.offline.then_some(true),
            proxy: cli.proxy.clone(),
            local_root: cli.local_root.as_ref().map(|root| {
                if root.is_absolute() {
                    root.clone()
                } else {
                    cwd.join(root)
                }
            }),
        };
        let mut loader = ConfigLoader::new(&cwd).with_overrides(overrides);
        if let Some(path) = &cli.config {
            loader = loader.with_config_path(path);
        }
        let loaded_config = loader.load().map_err(AppError::new)?;
        let sources = ConfigSources {
            offline: source(
                cli.offline,
                loaded_config.has_environment_key("FIXER_OFFLINE"),
                loaded_config.has_file_field("offline"),
            ),
            proxy: source(
                cli.proxy.is_some(),
                loaded_config.has_environment_key("FIXER_PROXY"),
                loaded_config.has_file_field("proxy"),
            ),
            local_root: source(
                cli.local_root.is_some(),
                loaded_config.has_environment_key("FIXER_LOCAL_ROOT"),
                loaded_config.has_file_field("local_root"),
            ),
            api_key: source(
                false,
                has_environment_key(
                    &loaded_config,
                    &[
                        "TMDB_API_TOKEN",
                        "FIXER_API_KEY",
                        "FIXER_PROVIDERS__TMDB__API_TOKEN",
                        "FIXER_PROVIDERS__TMDB__API_TOKEN_ENV",
                    ],
                ),
                has_file_field(
                    &loaded_config,
                    &["providers.tmdb.api_token", "providers.tmdb.api_token_env"],
                ),
            ),
            anilist: source(
                false,
                has_environment_key(
                    &loaded_config,
                    &["FIXER_ANILIST_ENABLED", "FIXER_ENABLED_PROVIDERS"],
                ),
                loaded_config.has_file_field("enabled_providers"),
            ),
        };

        let shared = loaded_config.config().clone();
        Ok(Self { shared, sources })
    }

    pub fn validate(&self) -> AppResult<()> {
        self.shared.validate().map_err(AppError::new)
    }

    pub const fn shared(&self) -> &FixerConfig {
        &self.shared
    }

    pub fn validation_summary(&self) -> String {
        let defaults = FixerConfig::default();
        let locales = self.shared.preferred_locales.join(",");
        let providers = self.shared.enabled_providers.join(",");
        format!(
            "configuration valid\noffline: {} ({})\nproxy: {} ({})\nlocal_root: {} ({})\napi_key: {} ({})\npreferred_locales: {}\ntimeout_seconds: {}\nauto_accept_confidence: {}\nreview_confidence: {}\noutput_preset: {}\nplacement: {}\nconflict_policy: {}\nenabled_providers: {}\ntmdb_secret: {}\nanilist_secret: {}\nopenlibrary_api: {}\nopenlibrary_cover: {}\nanilist: {} ({})\n",
            self.shared.offline,
            self.sources.offline,
            configured(self.shared.proxy.as_ref()),
            self.sources.proxy,
            configured(self.shared.local_root.as_ref()),
            self.sources.local_root,
            configured(self.shared.providers.tmdb.resolved_api_token()),
            self.sources.api_key,
            locales,
            self.shared.timeout_seconds,
            self.shared.auto_accept_confidence,
            self.shared.review_confidence,
            self.shared.output_preset,
            self.shared.placement,
            self.shared.conflict_policy,
            providers,
            configured_bool(self.shared.providers.tmdb.api_token_env.is_some()),
            configured_bool(self.shared.providers.anilist.access_token_env.is_some()),
            configured_bool(
                self.shared.providers.openlibrary.base_url
                    != defaults.providers.openlibrary.base_url,
            ),
            configured_bool(
                self.shared.providers.openlibrary.cover_base_url
                    != defaults.providers.openlibrary.cover_base_url,
            ),
            if self.shared.provider_enabled("anilist") {
                "enabled"
            } else {
                "disabled"
            },
            self.sources.anilist,
        )
    }
}

impl Deref for Config {
    type Target = FixerConfig;

    fn deref(&self) -> &Self::Target {
        &self.shared
    }
}

impl fmt::Debug for Config {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Config")
            .field("shared", &self.shared)
            .field("sources", &self.sources)
            .finish()
    }
}

fn has_environment_key(loaded: &LoadedConfig, keys: &[&str]) -> bool {
    keys.iter().any(|key| loaded.has_environment_key(key))
}

fn has_file_field(loaded: &LoadedConfig, fields: &[&str]) -> bool {
    fields.iter().any(|field| loaded.has_file_field(field))
}

const fn source(flag: bool, environment: bool, file: bool) -> ConfigSource {
    if flag {
        ConfigSource::Flag
    } else if environment {
        ConfigSource::Environment
    } else if file {
        ConfigSource::File
    } else {
        ConfigSource::Default
    }
}

const fn configured<T: ?Sized>(value: Option<&T>) -> &'static str {
    if value.is_some() {
        "configured"
    } else {
        "not configured"
    }
}

const fn configured_bool(value: bool) -> &'static str {
    if value {
        "configured"
    } else {
        "not configured"
    }
}
