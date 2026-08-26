use crate::{AppError, AppResult};
use fixer_core::{Confidence, LanguageTag, ProviderId};
use serde::Deserialize;
use std::{
    env, fmt, fs,
    path::{Path, PathBuf},
    str::FromStr,
    time::Duration,
};

const KNOWN_PROVIDERS: &[&str] = &[
    "local",
    "tmdb",
    "bangumi",
    "anilist",
    "musicbrainz",
    "openlibrary",
];
const DEFAULT_PROVIDERS: &[&str] = &["local", "tmdb", "bangumi", "musicbrainz", "openlibrary"];
const DEFAULT_LOCALES: &[&str] = &["zh-Hans", "zh-Hant", "ja", "en", "und"];

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutputPreset {
    Metadata,
    Full,
}
impl fmt::Display for OutputPreset {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Metadata => "metadata",
            Self::Full => "full",
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlacementPolicy {
    InPlace,
    Symlink,
    Hardlink,
    Copy,
    Reflink,
}
impl fmt::Display for PlacementPolicy {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InPlace => "in_place",
            Self::Symlink => "symlink",
            Self::Hardlink => "hardlink",
            Self::Copy => "copy",
            Self::Reflink => "reflink",
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConflictPolicy {
    PreferFirst,
    Review,
    Error,
}
impl fmt::Display for ConflictPolicy {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::PreferFirst => "prefer_first",
            Self::Review => "review",
            Self::Error => "error",
        })
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct SecretReferences {
    tmdb_api_token: Option<String>,
    anilist_access_token: Option<String>,
}

#[derive(Clone)]
pub struct Config {
    pub offline: bool,
    pub proxy: Option<String>,
    pub local_root: Option<PathBuf>,
    pub preferred_locales: Vec<LanguageTag>,
    pub timeout: Duration,
    pub auto_accept_confidence: Confidence,
    pub review_confidence: Confidence,
    pub output_preset: OutputPreset,
    pub placement: PlacementPolicy,
    pub conflict_policy: ConflictPolicy,
    enabled_providers: Vec<ProviderId>,
    api_key: Option<String>,
    tmdb_base_url: Option<String>,
    bangumi_base_url: Option<String>,
    musicbrainz_base_url: Option<String>,
    openlibrary_base_url: Option<String>,
    openlibrary_cover_base_url: Option<String>,
    anilist_enabled: bool,
    anilist_endpoint: Option<String>,
    anilist_access_token: Option<String>,
    tmdb_secret_referenced: bool,
    anilist_secret_referenced: bool,
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
    preferred_locales: Option<Vec<String>>,
    timeout_seconds: Option<u64>,
    auto_accept_confidence: Option<f32>,
    review_confidence: Option<f32>,
    output_preset: Option<OutputPreset>,
    placement: Option<PlacementPolicy>,
    conflict_policy: Option<ConflictPolicy>,
    enabled_providers: Option<Vec<String>>,
    secret_references: Option<SecretReferences>,
    #[serde(alias = "tmdb_api_token")]
    api_key: Option<String>,
    tmdb_base_url: Option<String>,
    bangumi_base_url: Option<String>,
    musicbrainz_base_url: Option<String>,
    openlibrary_base_url: Option<String>,
    openlibrary_cover_base_url: Option<String>,
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

        let preferred_locales = env::var("FIXER_PREFERRED_LOCALES")
            .ok()
            .map(|value| split_csv(&value))
            .or(file.preferred_locales)
            .unwrap_or_else(|| {
                DEFAULT_LOCALES
                    .iter()
                    .map(|value| (*value).to_owned())
                    .collect()
            })
            .into_iter()
            .map(|value| {
                value.parse::<LanguageTag>().map_err(|error| {
                    AppError::new(format!(
                        "preferred_locales contains invalid BCP 47 tag: {error}"
                    ))
                })
            })
            .collect::<AppResult<Vec<_>>>()?;
        if preferred_locales.is_empty() {
            return Err(AppError::new("preferred_locales must not be empty"));
        }

        let timeout_seconds = env_value("FIXER_TIMEOUT_SECONDS")?
            .or(file.timeout_seconds)
            .unwrap_or(30);
        if timeout_seconds == 0 {
            return Err(AppError::new("timeout_seconds must be greater than zero"));
        }
        let auto_accept_confidence = Confidence::new(
            env_value("FIXER_AUTO_ACCEPT_CONFIDENCE")?
                .or(file.auto_accept_confidence)
                .unwrap_or(0.9),
        )
        .map_err(AppError::new)?;
        let review_confidence = Confidence::new(
            env_value("FIXER_REVIEW_CONFIDENCE")?
                .or(file.review_confidence)
                .unwrap_or(0.6),
        )
        .map_err(AppError::new)?;
        if review_confidence > auto_accept_confidence {
            return Err(AppError::new(
                "review_confidence must not exceed auto_accept_confidence",
            ));
        }

        let output_preset = env::var("FIXER_OUTPUT_PRESET")
            .ok()
            .map(|value| parse_output_preset(&value))
            .transpose()?
            .or(file.output_preset)
            .unwrap_or(OutputPreset::Full);
        let placement = env::var("FIXER_PLACEMENT")
            .ok()
            .map(|value| parse_placement(&value))
            .transpose()?
            .or(file.placement)
            .unwrap_or(PlacementPolicy::InPlace);
        let conflict_policy = env::var("FIXER_CONFLICT_POLICY")
            .ok()
            .map(|value| parse_conflict_policy(&value))
            .transpose()?
            .or(file.conflict_policy)
            .unwrap_or(ConflictPolicy::Review);

        let configured_providers = env::var("FIXER_ENABLED_PROVIDERS")
            .ok()
            .map(|value| split_csv(&value))
            .or(file.enabled_providers);
        let providers_explicit = configured_providers.is_some();
        let mut enabled_providers =
            validate_providers(configured_providers.unwrap_or_else(|| {
                DEFAULT_PROVIDERS
                    .iter()
                    .map(|provider| (*provider).to_owned())
                    .collect()
            }))?;

        let env_anilist_enabled = env::var("FIXER_ANILIST_ENABLED")
            .ok()
            .map(|value| parse_bool("FIXER_ANILIST_ENABLED", &value))
            .transpose()?;
        let (requested_anilist, anilist_source) = if let Some(enabled) = env_anilist_enabled {
            (enabled, ConfigSource::Environment)
        } else if let Some(enabled) = file.anilist_enabled {
            (enabled, ConfigSource::File)
        } else {
            (false, ConfigSource::Default)
        };
        if !providers_explicit
            && requested_anilist
            && !contains_provider(&enabled_providers, "anilist")
        {
            enabled_providers.push(ProviderId::new("anilist").map_err(AppError::new)?);
        }
        let anilist_enabled = contains_provider(&enabled_providers, "anilist");

        let references = file.secret_references.unwrap_or_default();
        let referenced_tmdb =
            referenced_secret("tmdb_api_token", references.tmdb_api_token.as_deref())?;
        let referenced_anilist = referenced_secret(
            "anilist_access_token",
            references.anilist_access_token.as_deref(),
        )?;
        let direct_api_key = env::var("TMDB_API_TOKEN")
            .ok()
            .or_else(|| env::var("FIXER_API_KEY").ok());
        let (api_key, api_key_source) = if let Some(value) = direct_api_key {
            (Some(value), ConfigSource::Environment)
        } else if let Some(value) = referenced_tmdb {
            (Some(value), ConfigSource::File)
        } else if let Some(value) = file.api_key {
            (Some(value), ConfigSource::File)
        } else {
            (None, ConfigSource::Default)
        };
        let anilist_access_token = env::var("ANILIST_ACCESS_TOKEN")
            .ok()
            .or(referenced_anilist)
            .or(file.anilist_access_token);

        Ok(Self {
            offline,
            proxy,
            local_root,
            preferred_locales,
            timeout: Duration::from_secs(timeout_seconds),
            auto_accept_confidence,
            review_confidence,
            output_preset,
            placement,
            conflict_policy,
            enabled_providers,
            api_key,
            tmdb_base_url: env::var("TMDB_BASE_URL").ok().or(file.tmdb_base_url),
            bangumi_base_url: env::var("BANGUMI_BASE_URL").ok().or(file.bangumi_base_url),
            musicbrainz_base_url: env::var("MUSICBRAINZ_BASE_URL")
                .ok()
                .or(file.musicbrainz_base_url),
            openlibrary_base_url: env::var("OPENLIBRARY_BASE_URL")
                .ok()
                .or(file.openlibrary_base_url),
            openlibrary_cover_base_url: env::var("OPENLIBRARY_COVER_BASE_URL")
                .ok()
                .or(file.openlibrary_cover_base_url),
            anilist_enabled,
            anilist_endpoint: env::var("ANILIST_ENDPOINT").ok().or(file.anilist_endpoint),
            anilist_access_token,
            tmdb_secret_referenced: references.tmdb_api_token.is_some(),
            anilist_secret_referenced: references.anilist_access_token.is_some(),
            anilist_source,
            offline_source,
            proxy_source,
            local_root_source,
            api_key_source,
        })
    }

    pub fn provider_enabled(&self, provider: &str) -> bool {
        contains_provider(&self.enabled_providers, provider)
    }

    pub fn tmdb_provider(&self) -> AppResult<Option<fixer_provider_tmdb::TmdbProvider>> {
        if !self.provider_enabled("tmdb") {
            return Ok(None);
        }
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

    pub fn bangumi_provider(&self) -> AppResult<Option<fixer_provider_bangumi::BangumiProvider>> {
        if !self.provider_enabled("bangumi") {
            return Ok(None);
        }
        let mut config = fixer_provider_bangumi::BangumiConfig::default();
        if let Some(base_url) = &self.bangumi_base_url {
            config = config.with_base_url(base_url).map_err(AppError::new)?;
        }
        fixer_provider_bangumi::BangumiProvider::new(config)
            .map(Some)
            .map_err(AppError::new)
    }

    pub fn musicbrainz_provider(
        &self,
    ) -> AppResult<Option<fixer_provider_musicbrainz::MusicBrainzProvider>> {
        if !self.provider_enabled("musicbrainz") {
            return Ok(None);
        }
        let mut config = fixer_provider_musicbrainz::MusicBrainzConfig::default();
        if let Some(base_url) = &self.musicbrainz_base_url {
            config = config.with_base_url(base_url).map_err(AppError::new)?;
        }
        fixer_provider_musicbrainz::MusicBrainzProvider::new(config)
            .map(Some)
            .map_err(AppError::new)
    }

    pub fn openlibrary_provider(
        &self,
    ) -> AppResult<Option<fixer_provider_openlibrary::OpenLibraryProvider>> {
        if !self.provider_enabled("openlibrary") {
            return Ok(None);
        }
        let mut config = fixer_provider_openlibrary::OpenLibraryConfig::default();
        if let Some(base_url) = &self.openlibrary_base_url {
            config = config.with_api_base_url(base_url).map_err(AppError::new)?;
        }
        if let Some(base_url) = &self.openlibrary_cover_base_url {
            config = config
                .with_cover_base_url(base_url)
                .map_err(AppError::new)?;
        }
        fixer_provider_openlibrary::OpenLibraryProvider::new(config)
            .map(Some)
            .map_err(AppError::new)
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
        let locales = self
            .preferred_locales
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(",");
        let providers = self
            .enabled_providers
            .iter()
            .map(ProviderId::as_str)
            .collect::<Vec<_>>()
            .join(",");
        format!(
            "configuration valid\noffline: {} ({})\nproxy: {} ({})\nlocal_root: {} ({})\napi_key: {} ({})\npreferred_locales: {}\ntimeout_seconds: {}\nauto_accept_confidence: {}\nreview_confidence: {}\noutput_preset: {}\nplacement: {}\nconflict_policy: {}\nenabled_providers: {}\ntmdb_secret: {}\nanilist_secret: {}\nopenlibrary_api: {}\nopenlibrary_cover: {}\nanilist: {} ({})\n",
            self.offline,
            self.offline_source,
            configured(&self.proxy),
            self.proxy_source,
            configured(&self.local_root),
            self.local_root_source,
            configured(&self.api_key),
            self.api_key_source,
            locales,
            self.timeout.as_secs(),
            self.auto_accept_confidence.get(),
            self.review_confidence.get(),
            self.output_preset,
            self.placement,
            self.conflict_policy,
            providers,
            configured_bool(self.tmdb_secret_referenced),
            configured_bool(self.anilist_secret_referenced),
            configured(&self.openlibrary_base_url),
            configured(&self.openlibrary_cover_base_url),
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
            .field("preferred_locales", &self.preferred_locales)
            .field("timeout", &self.timeout)
            .field("auto_accept_confidence", &self.auto_accept_confidence)
            .field("review_confidence", &self.review_confidence)
            .field("output_preset", &self.output_preset)
            .field("placement", &self.placement)
            .field("conflict_policy", &self.conflict_policy)
            .field("enabled_providers", &self.enabled_providers)
            .field("api_key", &self.api_key.as_ref().map(|_| "[REDACTED]"))
            .field("tmdb_base_url", &self.tmdb_base_url)
            .field("bangumi_base_url", &self.bangumi_base_url)
            .field("musicbrainz_base_url", &self.musicbrainz_base_url)
            .field(
                "openlibrary_base_url",
                &self.openlibrary_base_url.as_ref().map(|_| "[CONFIGURED]"),
            )
            .field(
                "openlibrary_cover_base_url",
                &self
                    .openlibrary_cover_base_url
                    .as_ref()
                    .map(|_| "[CONFIGURED]"),
            )
            .field("anilist_enabled", &self.anilist_enabled)
            .field(
                "anilist_endpoint",
                &self.anilist_endpoint.as_ref().map(|_| "[CONFIGURED]"),
            )
            .field(
                "anilist_access_token",
                &self.anilist_access_token.as_ref().map(|_| "[REDACTED]"),
            )
            .field("tmdb_secret_referenced", &self.tmdb_secret_referenced)
            .field("anilist_secret_referenced", &self.anilist_secret_referenced)
            .finish()
    }
}

fn read_file(path: &Path) -> AppResult<FileConfig> {
    let input = fs::read_to_string(path).map_err(AppError::new)?;
    serde_json::from_str(&input).map_err(AppError::new)
}

fn validate_providers(values: Vec<String>) -> AppResult<Vec<ProviderId>> {
    if values.is_empty() {
        return Err(AppError::new("enabled_providers must not be empty"));
    }
    let mut providers = Vec::new();
    for value in values {
        if !KNOWN_PROVIDERS.contains(&value.as_str()) {
            return Err(AppError::new(format!("unknown provider `{value}`")));
        }
        let provider = ProviderId::new(value).map_err(AppError::new)?;
        if !providers.contains(&provider) {
            providers.push(provider);
        }
    }
    Ok(providers)
}

fn contains_provider(providers: &[ProviderId], expected: &str) -> bool {
    providers
        .iter()
        .any(|provider| provider.as_str() == expected)
}

fn referenced_secret(field: &str, reference: Option<&str>) -> AppResult<Option<String>> {
    let Some(reference) = reference else {
        return Ok(None);
    };
    if reference.is_empty()
        || !reference.chars().all(|character| {
            character.is_ascii_uppercase() || character.is_ascii_digit() || character == '_'
        })
    {
        return Err(AppError::new(format!(
            "secret reference `{field}` must name an uppercase environment variable"
        )));
    }
    env::var(reference).map(Some).map_err(|_| {
        AppError::new(format!(
            "secret reference `{field}` points to unset environment variable `{reference}`"
        ))
    })
}

fn env_value<T>(name: &str) -> AppResult<Option<T>>
where
    T: FromStr,
    T::Err: fmt::Display,
{
    env::var(name)
        .ok()
        .map(|value| {
            value
                .parse::<T>()
                .map_err(|error| AppError::new(format!("{name} is invalid: {error}")))
        })
        .transpose()
}

fn parse_bool(name: &str, value: &str) -> AppResult<bool> {
    match value.to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Ok(true),
        "0" | "false" | "no" | "off" => Ok(false),
        _ => Err(AppError::new(format!("{name} must be a boolean"))),
    }
}

fn parse_output_preset(value: &str) -> AppResult<OutputPreset> {
    match value {
        "metadata" => Ok(OutputPreset::Metadata),
        "full" => Ok(OutputPreset::Full),
        _ => Err(AppError::new(
            "FIXER_OUTPUT_PRESET must be metadata or full",
        )),
    }
}

fn parse_placement(value: &str) -> AppResult<PlacementPolicy> {
    match value {
        "in_place" | "in-place" => Ok(PlacementPolicy::InPlace),
        "symlink" => Ok(PlacementPolicy::Symlink),
        "hardlink" => Ok(PlacementPolicy::Hardlink),
        "copy" => Ok(PlacementPolicy::Copy),
        "reflink" => Ok(PlacementPolicy::Reflink),
        _ => Err(AppError::new("FIXER_PLACEMENT is invalid")),
    }
}

fn parse_conflict_policy(value: &str) -> AppResult<ConflictPolicy> {
    match value {
        "prefer_first" | "prefer-first" => Ok(ConflictPolicy::PreferFirst),
        "review" => Ok(ConflictPolicy::Review),
        "error" => Ok(ConflictPolicy::Error),
        _ => Err(AppError::new("FIXER_CONFLICT_POLICY is invalid")),
    }
}

fn split_csv(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(ToOwned::to_owned)
        .collect()
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

fn configured_bool(value: bool) -> &'static str {
    if value {
        "configured"
    } else {
        "not configured"
    }
}
