use std::{
    collections::{BTreeMap, HashSet},
    env, fmt, fs,
    io::{self, Write},
    net::SocketAddr,
    path::{Path, PathBuf},
    str::FromStr,
    sync::{Arc, Mutex, RwLock},
};

use config::{Config as ConfigBuilder, Environment, File, Map, Source, Value, ValueKind};
use fixer_core::{LanguageTag, ProviderId};
use serde::{Deserialize, Serialize};
use thiserror::Error;

const DEFAULT_LOCALES: &[&str] = &["zh-Hans", "zh-Hant", "ja", "en", "und"];
const DEFAULT_PROVIDERS: &[&str] = &["local", "tmdb", "bangumi", "musicbrainz", "openlibrary"];
const KNOWN_PROVIDERS: &[&str] = &[
    "local",
    "tmdb",
    "bangumi",
    "anilist",
    "musicbrainz",
    "openlibrary",
];
const DEFAULT_BIND: &str = "127.0.0.1:3000";
const DEFAULT_CONFIG_FILE: &str = "fixer.toml";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum OutputPreset {
    Metadata,
    #[default]
    Full,
}
impl fmt::Display for OutputPreset {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Metadata => "metadata",
            Self::Full => "full",
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum PlacementPolicy {
    #[default]
    #[serde(alias = "in-place")]
    InPlace,
    Symlink,
    Hardlink,
    Copy,
    Reflink,
}
impl fmt::Display for PlacementPolicy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InPlace => "in_place",
            Self::Symlink => "symlink",
            Self::Hardlink => "hardlink",
            Self::Copy => "copy",
            Self::Reflink => "reflink",
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum ConflictPolicy {
    #[serde(alias = "prefer-first")]
    PreferFirst,
    #[default]
    Review,
    Error,
}
impl fmt::Display for ConflictPolicy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::PreferFirst => "prefer_first",
            Self::Review => "review",
            Self::Error => "error",
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum LoggingFormat {
    #[default]
    Pretty,
    Json,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SecretString(String);
impl SecretString {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
    pub fn expose_secret(&self) -> &str {
        &self.0
    }
}
impl fmt::Debug for SecretString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("[REDACTED]")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct LoggingConfig {
    pub filter: String,
    pub format: LoggingFormat,
}
impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            filter: "fixer_server=info,tower_http=info".to_owned(),
            format: LoggingFormat::Pretty,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct TrustedProxyConfig {
    pub ranges: Vec<String>,
    pub header: String,
}
impl Default for TrustedProxyConfig {
    fn default() -> Self {
        Self {
            ranges: Vec::new(),
            header: "x-forwarded-for".to_owned(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ServerConfig {
    pub bind: SocketAddr,
    pub database: PathBuf,
    pub media_roots: Vec<PathBuf>,
    pub web_root: PathBuf,
    pub allowed_origins: Vec<String>,
    pub https_termination: bool,
    pub worker_count: usize,
    pub trusted_proxy: TrustedProxyConfig,
}
impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            bind: DEFAULT_BIND.parse().expect("valid default bind"),
            database: "fixer.sqlite3".into(),
            media_roots: Vec::new(),
            web_root: "web/dist".into(),
            allowed_origins: Vec::new(),
            https_termination: false,
            worker_count: 2,
            trusted_proxy: TrustedProxyConfig::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct EndpointConfig {
    pub base_url: String,
}
impl EndpointConfig {
    fn new(base_url: &str) -> Self {
        Self {
            base_url: base_url.to_owned(),
        }
    }
}
impl Default for EndpointConfig {
    fn default() -> Self {
        Self::new("")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct OpenLibraryConfig {
    pub base_url: String,
    pub cover_base_url: String,
}
impl Default for OpenLibraryConfig {
    fn default() -> Self {
        Self {
            base_url: "https://openlibrary.org".to_owned(),
            cover_base_url: "https://covers.openlibrary.org/b/".to_owned(),
        }
    }
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct TmdbConfig {
    pub base_url: String,
    pub api_token: Option<SecretString>,
    pub api_token_env: Option<String>,
    #[serde(skip)]
    resolved_api_token: Option<SecretString>,
}
impl Default for TmdbConfig {
    fn default() -> Self {
        Self {
            base_url: "https://api.themoviedb.org/3".to_owned(),
            api_token: None,
            api_token_env: None,
            resolved_api_token: None,
        }
    }
}
impl fmt::Debug for TmdbConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TmdbConfig")
            .field("base_url", &self.base_url)
            .field("api_token", &self.api_token)
            .field("api_token_env", &self.api_token_env)
            .field("resolved_api_token", &self.resolved_api_token)
            .finish()
    }
}
impl TmdbConfig {
    pub fn resolved_api_token(&self) -> Option<&str> {
        self.resolved_api_token
            .as_ref()
            .or(self.api_token.as_ref())
            .map(SecretString::expose_secret)
    }
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AniListConfig {
    pub base_url: String,
    pub access_token: Option<SecretString>,
    pub access_token_env: Option<String>,
    #[serde(skip)]
    resolved_access_token: Option<SecretString>,
}
impl Default for AniListConfig {
    fn default() -> Self {
        Self {
            base_url: "https://graphql.anilist.co".to_owned(),
            access_token: None,
            access_token_env: None,
            resolved_access_token: None,
        }
    }
}
impl fmt::Debug for AniListConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AniListConfig")
            .field("base_url", &self.base_url)
            .field("access_token", &self.access_token)
            .field("access_token_env", &self.access_token_env)
            .field("resolved_access_token", &self.resolved_access_token)
            .finish()
    }
}
impl AniListConfig {
    pub fn resolved_access_token(&self) -> Option<&str> {
        self.resolved_access_token
            .as_ref()
            .or(self.access_token.as_ref())
            .map(SecretString::expose_secret)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ProvidersConfig {
    pub tmdb: TmdbConfig,
    pub bangumi: EndpointConfig,
    pub anilist: AniListConfig,
    pub musicbrainz: EndpointConfig,
    pub openlibrary: OpenLibraryConfig,
}

pub type ProviderEndpoints = ProvidersConfig;

impl Default for ProvidersConfig {
    fn default() -> Self {
        Self {
            tmdb: TmdbConfig::default(),
            bangumi: EndpointConfig::new("https://api.bgm.tv"),
            anilist: AniListConfig::default(),
            musicbrainz: EndpointConfig::new("https://musicbrainz.org/ws/2"),
            openlibrary: OpenLibraryConfig::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct FixerConfig {
    pub offline: bool,
    pub proxy: Option<String>,
    pub local_root: Option<PathBuf>,
    pub preferred_locales: Vec<String>,
    pub timeout_seconds: u64,
    pub auto_accept_confidence: f32,
    pub review_confidence: f32,
    pub output_preset: OutputPreset,
    pub placement: PlacementPolicy,
    pub conflict_policy: ConflictPolicy,
    pub enabled_providers: Vec<String>,
    pub providers: ProvidersConfig,
    pub server: ServerConfig,
    pub logging: LoggingConfig,
}
impl Default for FixerConfig {
    fn default() -> Self {
        Self {
            offline: false,
            proxy: None,
            local_root: None,
            preferred_locales: DEFAULT_LOCALES.iter().map(|v| (*v).to_owned()).collect(),
            timeout_seconds: 30,
            auto_accept_confidence: 0.9,
            review_confidence: 0.6,
            output_preset: OutputPreset::Full,
            placement: PlacementPolicy::InPlace,
            conflict_policy: ConflictPolicy::Review,
            enabled_providers: DEFAULT_PROVIDERS.iter().map(|v| (*v).to_owned()).collect(),
            providers: ProvidersConfig::default(),
            server: ServerConfig::default(),
            logging: LoggingConfig::default(),
        }
    }
}
impl FixerConfig {
    pub fn enabled_provider_names(&self) -> Vec<&str> {
        self.enabled_providers.iter().map(String::as_str).collect()
    }
    pub fn provider_enabled(&self, provider: &str) -> bool {
        self.enabled_providers.iter().any(|value| value == provider)
    }
    pub fn validate(&self) -> Result<(), ConfigLoadError> {
        if self.preferred_locales.is_empty() {
            return Err(ConfigLoadError::Validation(
                "preferred_locales must not be empty".to_owned(),
            ));
        }
        for locale in &self.preferred_locales {
            LanguageTag::from_str(locale).map_err(|e| {
                ConfigLoadError::Validation(format!(
                    "preferred_locales contains invalid BCP 47 tag: {e}"
                ))
            })?;
        }
        if self.timeout_seconds == 0 {
            return Err(ConfigLoadError::Validation(
                "timeout_seconds must be greater than zero".to_owned(),
            ));
        }
        for (name, value) in [
            ("auto_accept_confidence", self.auto_accept_confidence),
            ("review_confidence", self.review_confidence),
        ] {
            if !value.is_finite() || !(0.0..=1.0).contains(&value) {
                return Err(ConfigLoadError::Validation(format!(
                    "{name} must be finite and between 0 and 1"
                )));
            }
        }
        if self.review_confidence > self.auto_accept_confidence {
            return Err(ConfigLoadError::Validation(
                "review_confidence must not exceed auto_accept_confidence".to_owned(),
            ));
        }
        if self.enabled_providers.is_empty() {
            return Err(ConfigLoadError::Validation(
                "enabled_providers must not be empty".to_owned(),
            ));
        }
        for provider in &self.enabled_providers {
            ProviderId::new(provider)
                .map_err(|error| ConfigLoadError::Validation(error.to_string()))?;
            if !KNOWN_PROVIDERS.contains(&provider.as_str()) {
                return Err(ConfigLoadError::Validation(format!(
                    "unknown provider `{provider}`"
                )));
            }
        }
        if self.server.worker_count == 0 {
            return Err(ConfigLoadError::Validation(
                "server.worker_count must be greater than zero".to_owned(),
            ));
        }
        validate_secret_reference(
            "providers.tmdb.api_token_env",
            self.providers.tmdb.api_token_env.as_deref(),
        )?;
        validate_secret_reference(
            "providers.anilist.access_token_env",
            self.providers.anilist.access_token_env.as_deref(),
        )?;
        validate_provider_endpoints(&self.providers)?;
        validate_proxy(self.proxy.as_deref())?;
        validate_allowed_origins(&self.server.allowed_origins)?;
        validate_trusted_proxy(&self.server.trusted_proxy)?;
        tracing_subscriber::EnvFilter::try_new(&self.logging.filter)
            .map_err(|error| invalid_field("logging.filter", error))?;
        Ok(())
    }
    fn normalize(&mut self, base: &Path) -> Result<(), ConfigLoadError> {
        if let Some(root) = &mut self.local_root {
            let path = if root.is_absolute() {
                root.clone()
            } else {
                base.join(&*root)
            };
            *root = path
                .canonicalize()
                .map_err(|source| ConfigLoadError::LocalRoot { path, source })?;
            if !root.is_dir() {
                return Err(ConfigLoadError::Validation(format!(
                    "local_root is not a directory: {}",
                    root.display()
                )));
            }
        }
        if self.server.database.is_relative() {
            self.server.database = base.join(&self.server.database);
        }
        if self.server.web_root.is_relative() {
            self.server.web_root = base.join(&self.server.web_root);
        }
        let mut seen = HashSet::new();
        self.enabled_providers.retain(|p| seen.insert(p.clone()));
        for root in &mut self.server.media_roots {
            let path = if root.is_absolute() {
                root.clone()
            } else {
                base.join(&*root)
            };
            *root = path
                .canonicalize()
                .map_err(|source| ConfigLoadError::MediaRoot { path, source })?;
            if !root.is_dir() {
                return Err(ConfigLoadError::Validation(format!(
                    "server.media_roots entry is not a directory: {}",
                    root.display()
                )));
            }
        }
        self.validate()
    }
    fn resolve_secrets(
        &mut self,
        environment: &BTreeMap<String, String>,
    ) -> Result<(), ConfigLoadError> {
        self.providers.tmdb.resolved_api_token = environment
            .get("TMDB_API_TOKEN")
            .or_else(|| environment.get("FIXER_API_KEY"))
            .or_else(|| environment.get("FIXER_PROVIDERS__TMDB__API_TOKEN"))
            .map(|v| SecretString::new(v.clone()))
            .or(resolve_reference(
                "providers.tmdb.api_token_env",
                self.providers.tmdb.api_token_env.as_deref(),
                environment,
            )?);
        self.providers.anilist.resolved_access_token = environment
            .get("ANILIST_ACCESS_TOKEN")
            .or_else(|| environment.get("FIXER_PROVIDERS__ANILIST__ACCESS_TOKEN"))
            .map(|v| SecretString::new(v.clone()))
            .or(resolve_reference(
                "providers.anilist.access_token_env",
                self.providers.anilist.access_token_env.as_deref(),
                environment,
            )?);
        Ok(())
    }
}

#[derive(Clone)]
pub struct LoadedConfig {
    path: PathBuf,
    base: PathBuf,
    environment: BTreeMap<String, String>,
    file_fields: HashSet<String>,
    config: FixerConfig,
}
impl fmt::Debug for LoadedConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LoadedConfig")
            .field("path", &self.path)
            .field("config", &self.config)
            .finish()
    }
}
impl LoadedConfig {
    pub fn path(&self) -> &Path {
        &self.path
    }
    pub fn config(&self) -> &FixerConfig {
        &self.config
    }
    pub fn has_environment_key(&self, key: &str) -> bool {
        self.environment.contains_key(key)
    }
    pub fn has_file_field(&self, field: &str) -> bool {
        self.file_fields.contains(field)
    }
    pub fn into_handle(self) -> ConfigHandle {
        ConfigHandle {
            path: Arc::new(self.path),
            base: Arc::new(self.base),
            environment: Arc::new(self.environment),
            persistence: Arc::new(Mutex::new(())),
            config: Arc::new(RwLock::new(self.config)),
        }
    }
}

#[derive(Clone)]
pub struct ConfigHandle {
    path: Arc<PathBuf>,
    base: Arc<PathBuf>,
    environment: Arc<BTreeMap<String, String>>,
    persistence: Arc<Mutex<()>>,
    config: Arc<RwLock<FixerConfig>>,
}
impl fmt::Debug for ConfigHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ConfigHandle")
            .field("path", &self.path)
            .field("config", &self.snapshot())
            .finish()
    }
}
impl ConfigHandle {
    pub fn path(&self) -> &Path {
        self.path.as_ref()
    }
    pub fn snapshot(&self) -> FixerConfig {
        self.config
            .read()
            .expect("configuration lock is not poisoned")
            .clone()
    }
    pub fn replace_and_persist(&self, next: FixerConfig) -> Result<(), ConfigWriteError> {
        if !matches!(
            self.path.extension().and_then(|extension| extension.to_str()),
            Some(extension) if extension.eq_ignore_ascii_case("toml")
        ) {
            return Err(ConfigWriteError::UnsupportedFormat {
                path: self.path.as_ref().clone(),
            });
        }
        self.replace_after(next, |next| self.persist_toml(next))
    }

    fn replace_after(
        &self,
        mut next: FixerConfig,
        persist: impl FnOnce(&FixerConfig) -> Result<(), ConfigWriteError>,
    ) -> Result<(), ConfigWriteError> {
        let _writer = self
            .persistence
            .lock()
            .expect("configuration persistence lock is not poisoned");
        next.resolve_secrets(&self.environment)
            .map_err(ConfigWriteError::Validation)?;
        next.normalize(&self.base)
            .map_err(ConfigWriteError::Validation)?;
        persist(&next)?;
        *self
            .config
            .write()
            .expect("configuration lock is not poisoned") = next;
        Ok(())
    }

    fn persist_toml(&self, next: &FixerConfig) -> Result<(), ConfigWriteError> {
        let serialized = toml::to_string_pretty(next)?;
        let parent = self.path.parent().unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(parent).map_err(|source| ConfigWriteError::Io {
            path: parent.to_owned(),
            source,
        })?;
        let temporary = temporary_path(&self.path);
        let result = (|| {
            let mut options = fs::OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            let mut file = options
                .open(&temporary)
                .map_err(|source| ConfigWriteError::Io {
                    path: temporary.clone(),
                    source,
                })?;
            file.write_all(serialized.as_bytes())
                .and_then(|_| file.sync_all())
                .map_err(|source| ConfigWriteError::Io {
                    path: temporary.clone(),
                    source,
                })?;
            fs::rename(&temporary, self.path.as_ref()).map_err(|source| ConfigWriteError::Io {
                path: self.path.as_ref().clone(),
                source,
            })
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        result
    }
}

#[derive(Debug, Clone, Default)]
pub struct ConfigOverrides {
    pub offline: Option<bool>,
    pub proxy: Option<String>,
    pub local_root: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct ConfigLoader {
    cwd: PathBuf,
    environment: BTreeMap<String, String>,
    config_path: Option<PathBuf>,
    overrides: ConfigOverrides,
}
impl Default for ConfigLoader {
    fn default() -> Self {
        Self {
            cwd: env::current_dir().unwrap_or_else(|_| ".".into()),
            environment: env::vars().collect(),
            config_path: None,
            overrides: ConfigOverrides::default(),
        }
    }
}
impl ConfigLoader {
    pub fn new(cwd: impl AsRef<Path>) -> Self {
        Self {
            cwd: cwd.as_ref().to_owned(),
            environment: env::vars().collect(),
            config_path: None,
            overrides: ConfigOverrides::default(),
        }
    }
    pub fn with_environment(mut self, environment: BTreeMap<String, String>) -> Self {
        self.environment = environment;
        self
    }
    pub fn with_config_path(mut self, path: impl AsRef<Path>) -> Self {
        self.config_path = Some(path.as_ref().to_owned());
        self
    }
    pub fn with_overrides(mut self, overrides: ConfigOverrides) -> Self {
        self.overrides = overrides;
        self
    }
    pub fn load(mut self) -> Result<LoadedConfig, ConfigLoadError> {
        self.load_dotenv()?;
        let selected = self.selected_path();
        let explicit = self.config_path.is_some() || self.environment.contains_key("FIXER_CONFIG");
        if explicit && !selected.is_file() {
            return Err(ConfigLoadError::MissingConfig(selected));
        }
        let base = selected.parent().unwrap_or(&self.cwd).to_owned();
        let providers_explicit = explicit_provider_list(&selected, &self.environment)?;
        let secret_environment_keys = secret_environment_keys(&selected, &self.environment)?;
        let file_fields = if selected.is_file() {
            let values = CompatibleFile::new(selected.clone()).collect()?;
            field_paths(&values)
        } else {
            HashSet::new()
        };
        let mut builder = ConfigBuilder::builder();
        if selected.is_file() {
            builder = builder.add_source(CompatibleFile::new(selected.clone()));
        }
        builder = builder.add_source(
            Environment::with_prefix("FIXER")
                .prefix_separator("_")
                .separator("__")
                .list_separator(",")
                .with_list_parse_key("preferred_locales")
                .with_list_parse_key("enabled_providers")
                .with_list_parse_key("server.media_roots")
                .with_list_parse_key("server.allowed_origins")
                .with_list_parse_key("server.trusted_proxy.ranges")
                .try_parsing(true)
                .source(Some(filtered_environment(
                    &self.environment,
                    &secret_environment_keys,
                ))),
        );
        if let Some(offline) = self.overrides.offline {
            builder = builder.set_override("offline", offline)?;
        }
        if let Some(proxy) = &self.overrides.proxy {
            builder = builder.set_override("proxy", proxy.clone())?;
        }
        if let Some(local_root) = &self.overrides.local_root {
            builder =
                builder.set_override("local_root", local_root.to_string_lossy().into_owned())?;
        }
        let mut config: FixerConfig = builder.build()?.try_deserialize()?;
        apply_legacy_overrides(&mut config, &self.environment, providers_explicit)?;
        config.resolve_secrets(&self.environment)?;
        config.normalize(&base)?;
        Ok(LoadedConfig {
            path: selected,
            base,
            environment: self.environment,
            file_fields,
            config,
        })
    }
    fn load_dotenv(&mut self) -> Result<(), ConfigLoadError> {
        let path = self.cwd.join(".env");
        if !path.exists() {
            return Ok(());
        }
        let values = dotenvy::from_path_iter(&path).map_err(|source| ConfigLoadError::Dotenv {
            path: path.clone(),
            source,
        })?;
        for value in values {
            let (key, value) = value.map_err(|source| ConfigLoadError::Dotenv {
                path: path.clone(),
                source,
            })?;
            self.environment.entry(key).or_insert(value);
        }
        Ok(())
    }
    fn selected_path(&self) -> PathBuf {
        let path = self
            .config_path
            .clone()
            .or_else(|| self.environment.get("FIXER_CONFIG").map(PathBuf::from))
            .unwrap_or_else(|| DEFAULT_CONFIG_FILE.into());
        if path.is_absolute() {
            path
        } else {
            self.cwd.join(path)
        }
    }
}

#[derive(Debug, Clone)]
struct CompatibleFile {
    path: PathBuf,
}

impl CompatibleFile {
    fn new(path: PathBuf) -> Self {
        Self { path }
    }
}

impl Source for CompatibleFile {
    fn clone_into_box(&self) -> Box<dyn Source + Send + Sync> {
        Box::new(self.clone())
    }

    fn collect(&self) -> Result<Map<String, Value>, config::ConfigError> {
        let file = ConfigBuilder::builder()
            .add_source(File::from(self.path.clone()))
            .build()?;
        let mut values = file.collect()?;
        normalize_legacy_file(&mut values)?;
        Ok(values)
    }
}

fn normalize_legacy_file(values: &mut Map<String, Value>) -> Result<(), config::ConfigError> {
    move_legacy_value(
        values,
        &["api_key", "tmdb_api_token"],
        &["providers", "tmdb", "api_token"],
    );
    move_legacy_value(
        values,
        &["tmdb_base_url"],
        &["providers", "tmdb", "base_url"],
    );
    move_legacy_value(
        values,
        &["bangumi_base_url"],
        &["providers", "bangumi", "base_url"],
    );
    move_legacy_value(
        values,
        &["musicbrainz_base_url"],
        &["providers", "musicbrainz", "base_url"],
    );
    move_legacy_value(
        values,
        &["openlibrary_base_url"],
        &["providers", "openlibrary", "base_url"],
    );
    move_legacy_value(
        values,
        &["openlibrary_cover_base_url"],
        &["providers", "openlibrary", "cover_base_url"],
    );
    move_legacy_value(
        values,
        &["anilist_endpoint"],
        &["providers", "anilist", "base_url"],
    );
    move_legacy_value(
        values,
        &["anilist_token", "anilist_access_token"],
        &["providers", "anilist", "access_token"],
    );

    if let Some(enabled) = take_legacy_value(values, &["anilist_enabled"])
        && !contains_nested(values, &["enabled_providers"])
    {
        let enabled = enabled.into_bool()?;
        let mut providers = DEFAULT_PROVIDERS
            .iter()
            .map(|provider| (*provider).to_owned())
            .collect::<Vec<_>>();
        if enabled {
            providers.push("anilist".to_owned());
        }
        insert_nested_if_absent(values, &["enabled_providers"], Value::new(None, providers));
    }

    values.remove("bangumi_access_token");
    if let Some(references) = values.remove("secret_references") {
        match references.kind {
            ValueKind::Table(mut references) => {
                references.remove("bangumi_access_token");
                move_legacy_value(
                    &mut references,
                    &["tmdb_api_token"],
                    &["providers", "tmdb", "api_token_env"],
                );
                move_legacy_value(
                    &mut references,
                    &["anilist_access_token"],
                    &["providers", "anilist", "access_token_env"],
                );
                merge_nested(values, references);
            }
            kind => {
                values.insert("secret_references".to_owned(), Value::new(None, kind));
            }
        }
    }
    Ok(())
}

fn take_legacy_value(values: &mut Map<String, Value>, aliases: &[&str]) -> Option<Value> {
    let mut selected = None;
    for alias in aliases {
        let value = values.remove(*alias);
        if selected.is_none() {
            selected = value;
        }
    }
    selected
}

fn move_legacy_value(values: &mut Map<String, Value>, aliases: &[&str], target: &[&str]) {
    if let Some(value) = take_legacy_value(values, aliases) {
        insert_nested_if_absent(values, target, value);
    }
}

fn field_paths(values: &Map<String, Value>) -> HashSet<String> {
    fn collect(values: &Map<String, Value>, prefix: &str, fields: &mut HashSet<String>) {
        for (key, value) in values {
            let field = if prefix.is_empty() {
                key.clone()
            } else {
                format!("{prefix}.{key}")
            };
            match &value.kind {
                ValueKind::Table(table) => collect(table, &field, fields),
                _ => {
                    fields.insert(field);
                }
            }
        }
    }

    let mut fields = HashSet::new();
    collect(values, "", &mut fields);
    fields
}

fn contains_nested(values: &Map<String, Value>, path: &[&str]) -> bool {
    let Some((head, tail)) = path.split_first() else {
        return true;
    };
    let Some(value) = values.get(*head) else {
        return false;
    };
    if tail.is_empty() {
        return true;
    }
    match &value.kind {
        ValueKind::Table(table) => contains_nested(table, tail),
        _ => false,
    }
}

fn insert_nested_if_absent(values: &mut Map<String, Value>, path: &[&str], value: Value) {
    let Some((head, tail)) = path.split_first() else {
        return;
    };
    if tail.is_empty() {
        values.entry((*head).to_owned()).or_insert(value);
        return;
    }
    let entry = values
        .entry((*head).to_owned())
        .or_insert_with(|| Value::new(None, Map::<String, Value>::new()));
    if let ValueKind::Table(table) = &mut entry.kind {
        insert_nested_if_absent(table, tail, value);
    }
}

fn merge_nested(values: &mut Map<String, Value>, incoming: Map<String, Value>) {
    for (key, value) in incoming {
        match (values.get_mut(&key), value.kind) {
            (Some(existing), ValueKind::Table(incoming)) => {
                if let ValueKind::Table(existing) = &mut existing.kind {
                    merge_nested(existing, incoming);
                }
            }
            (None, kind) => {
                values.insert(key, Value::new(None, kind));
            }
            (Some(_), _) => {}
        }
    }
}

#[derive(Debug, Error)]
pub enum ConfigLoadError {
    #[error("failed to load {path}: {source}")]
    Dotenv {
        path: PathBuf,
        source: dotenvy::Error,
    },
    #[error("configuration file does not exist: {0}")]
    MissingConfig(PathBuf),
    #[error("configuration load failed: {0}")]
    Config(#[from] config::ConfigError),
    #[error("invalid configuration: {0}")]
    Validation(String),
    #[error("invalid local root {path}: {source}")]
    LocalRoot { path: PathBuf, source: io::Error },
    #[error("invalid media root {path}: {source}")]
    MediaRoot { path: PathBuf, source: io::Error },
}
#[derive(Debug, Error)]
pub enum ConfigWriteError {
    #[error("configuration persistence requires a TOML path: {path}")]
    UnsupportedFormat { path: PathBuf },
    #[error(transparent)]
    Validation(ConfigLoadError),
    #[error("failed to serialize configuration: {0}")]
    Serialize(#[from] toml::ser::Error),
    #[error("configuration I/O failed for {path}: {source}")]
    Io { path: PathBuf, source: io::Error },
}

fn secret_environment_keys(
    path: &Path,
    environment: &BTreeMap<String, String>,
) -> Result<HashSet<String>, ConfigLoadError> {
    let mut keys = HashSet::new();
    for variable in [
        "FIXER_PROVIDERS__TMDB__API_TOKEN_ENV",
        "FIXER_PROVIDERS__ANILIST__ACCESS_TOKEN_ENV",
    ] {
        if let Some(value) = environment.get(variable) {
            keys.insert(value.clone());
        }
    }
    if path.is_file() {
        let file = ConfigBuilder::builder()
            .add_source(File::from(path.to_owned()))
            .build()?;
        for field in [
            "providers.tmdb.api_token_env",
            "providers.anilist.access_token_env",
            "secret_references.tmdb_api_token",
            "secret_references.anilist_access_token",
        ] {
            if let Ok(value) = file.get_string(field) {
                keys.insert(value);
            }
        }
    }
    Ok(keys)
}

fn filtered_environment(
    environment: &BTreeMap<String, String>,
    secret_environment_keys: &HashSet<String>,
) -> Map<String, String> {
    const CANONICAL: &[&str] = &[
        "FIXER_OFFLINE",
        "FIXER_PROXY",
        "FIXER_LOCAL_ROOT",
        "FIXER_PREFERRED_LOCALES",
        "FIXER_TIMEOUT_SECONDS",
        "FIXER_AUTO_ACCEPT_CONFIDENCE",
        "FIXER_REVIEW_CONFIDENCE",
        "FIXER_OUTPUT_PRESET",
        "FIXER_PLACEMENT",
        "FIXER_CONFLICT_POLICY",
        "FIXER_ENABLED_PROVIDERS",
        "FIXER_PROVIDERS__TMDB__BASE_URL",
        "FIXER_PROVIDERS__TMDB__API_TOKEN",
        "FIXER_PROVIDERS__TMDB__API_TOKEN_ENV",
        "FIXER_PROVIDERS__BANGUMI__BASE_URL",
        "FIXER_PROVIDERS__ANILIST__BASE_URL",
        "FIXER_PROVIDERS__ANILIST__ACCESS_TOKEN",
        "FIXER_PROVIDERS__ANILIST__ACCESS_TOKEN_ENV",
        "FIXER_PROVIDERS__MUSICBRAINZ__BASE_URL",
        "FIXER_PROVIDERS__OPENLIBRARY__BASE_URL",
        "FIXER_PROVIDERS__OPENLIBRARY__COVER_BASE_URL",
        "FIXER_SERVER__BIND",
        "FIXER_SERVER__DATABASE",
        "FIXER_SERVER__MEDIA_ROOTS",
        "FIXER_SERVER__WEB_ROOT",
        "FIXER_SERVER__ALLOWED_ORIGINS",
        "FIXER_SERVER__HTTPS_TERMINATION",
        "FIXER_SERVER__WORKER_COUNT",
        "FIXER_SERVER__TRUSTED_PROXY__RANGES",
        "FIXER_SERVER__TRUSTED_PROXY__HEADER",
        "FIXER_LOGGING__FILTER",
        "FIXER_LOGGING__FORMAT",
    ];
    const EXCLUDED: &[&str] = &[
        "FIXER_CONFIG",
        "FIXER_API_KEY",
        "FIXER_PROVIDERS__TMDB__API_TOKEN",
        "FIXER_PROVIDERS__ANILIST__ACCESS_TOKEN",
        "FIXER_ANILIST_ENABLED",
        "FIXER_BANGUMI_ACCESS_TOKEN",
        "FIXER_SERVER_BIND",
        "FIXER_SERVER_DATABASE",
        "FIXER_SERVER_MEDIA_ROOTS",
        "FIXER_SERVER_HTTPS_TERMINATION",
        "FIXER_SERVER_ALLOWED_ORIGINS",
        "FIXER_SERVER_TRUSTED_PROXY_RANGES",
        "FIXER_SERVER_TRUSTED_PROXY_HEADER",
        "FIXER_SERVER_PASSWORD",
        "FIXER_WEB_ROOT",
    ];
    environment
        .iter()
        .filter(|(key, _)| {
            CANONICAL.contains(&key.as_str())
                && !EXCLUDED.contains(&key.as_str())
                && !secret_environment_keys.contains(*key)
        })
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect()
}
fn apply_legacy_overrides(
    config: &mut FixerConfig,
    environment: &BTreeMap<String, String>,
    providers_explicit: bool,
) -> Result<(), ConfigLoadError> {
    if let Some(v) = environment.get("FIXER_SERVER_BIND") {
        config.server.bind = v.parse().map_err(|e| {
            ConfigLoadError::Validation(format!("FIXER_SERVER_BIND is invalid: {e}"))
        })?;
    }
    if let Some(v) = environment.get("FIXER_SERVER_DATABASE") {
        config.server.database = v.into();
    }
    if let Some(v) = environment.get("FIXER_SERVER_MEDIA_ROOTS") {
        config.server.media_roots = env::split_paths(v).collect();
    }
    if let Some(v) = environment.get("FIXER_WEB_ROOT") {
        config.server.web_root = v.into();
    }
    if let Some(v) = environment.get("FIXER_SERVER_ALLOWED_ORIGINS") {
        config.server.allowed_origins = csv(v);
    }
    if let Some(v) = environment.get("FIXER_SERVER_HTTPS_TERMINATION") {
        config.server.https_termination = parse_bool("FIXER_SERVER_HTTPS_TERMINATION", v)?;
    }
    if let Some(v) = environment.get("FIXER_SERVER_TRUSTED_PROXY_RANGES") {
        config.server.trusted_proxy.ranges = csv(v);
    }
    if let Some(v) = environment.get("FIXER_SERVER_TRUSTED_PROXY_HEADER") {
        config.server.trusted_proxy.header = v.clone();
    }
    if let Some(v) = environment.get("TMDB_BASE_URL") {
        config.providers.tmdb.base_url = v.clone();
    }
    if let Some(v) = environment.get("BANGUMI_BASE_URL") {
        config.providers.bangumi.base_url = v.clone();
    }
    if let Some(v) = environment.get("MUSICBRAINZ_BASE_URL") {
        config.providers.musicbrainz.base_url = v.clone();
    }
    if let Some(v) = environment.get("OPENLIBRARY_BASE_URL") {
        config.providers.openlibrary.base_url = v.clone();
    }
    if let Some(v) = environment.get("OPENLIBRARY_COVER_BASE_URL") {
        config.providers.openlibrary.cover_base_url = v.clone();
    }
    if let Some(v) = environment.get("ANILIST_ENDPOINT") {
        config.providers.anilist.base_url = v.clone();
    }
    if let Some(value) = environment.get("FIXER_ANILIST_ENABLED") {
        let enabled = parse_bool("FIXER_ANILIST_ENABLED", value)?;
        if !providers_explicit {
            if enabled && !config.provider_enabled("anilist") {
                config.enabled_providers.push("anilist".to_owned());
            } else if !enabled {
                config
                    .enabled_providers
                    .retain(|provider| provider != "anilist");
            }
        }
    }
    Ok(())
}
fn explicit_provider_list(
    path: &Path,
    environment: &BTreeMap<String, String>,
) -> Result<bool, ConfigLoadError> {
    if environment.contains_key("FIXER_ENABLED_PROVIDERS") {
        return Ok(true);
    }
    if !path.is_file() {
        return Ok(false);
    }
    let file = ConfigBuilder::builder()
        .add_source(File::from(path.to_owned()))
        .build()?;
    Ok(file.get::<Vec<String>>("enabled_providers").is_ok())
}
fn validate_provider_endpoints(providers: &ProviderEndpoints) -> Result<(), ConfigLoadError> {
    for (field, endpoint) in [
        ("providers.tmdb.base_url", providers.tmdb.base_url.as_str()),
        (
            "providers.bangumi.base_url",
            providers.bangumi.base_url.as_str(),
        ),
        (
            "providers.anilist.base_url",
            providers.anilist.base_url.as_str(),
        ),
        (
            "providers.musicbrainz.base_url",
            providers.musicbrainz.base_url.as_str(),
        ),
        (
            "providers.openlibrary.base_url",
            providers.openlibrary.base_url.as_str(),
        ),
        (
            "providers.openlibrary.cover_base_url",
            providers.openlibrary.cover_base_url.as_str(),
        ),
    ] {
        validate_http_endpoint(field, endpoint)?;
    }

    fixer_provider_tmdb::TmdbConfig::new("configuration-validation-token")
        .and_then(|config| config.with_base_url(&providers.tmdb.base_url))
        .map_err(|error| invalid_field("providers.tmdb.base_url", error))?;
    fixer_provider_bangumi::BangumiConfig::default()
        .with_base_url(&providers.bangumi.base_url)
        .map_err(|error| invalid_field("providers.bangumi.base_url", error))?;
    fixer_provider_anilist::AniListConfig::default()
        .with_endpoint(&providers.anilist.base_url)
        .map_err(|error| invalid_field("providers.anilist.base_url", error))?;
    fixer_provider_musicbrainz::MusicBrainzConfig::default()
        .with_base_url(&providers.musicbrainz.base_url)
        .map_err(|error| invalid_field("providers.musicbrainz.base_url", error))?;
    fixer_provider_openlibrary::OpenLibraryConfig::default()
        .with_api_base_url(&providers.openlibrary.base_url)
        .map_err(|error| invalid_field("providers.openlibrary.base_url", error))?
        .with_cover_base_url(&providers.openlibrary.cover_base_url)
        .map_err(|error| invalid_field("providers.openlibrary.cover_base_url", error))?;
    Ok(())
}

fn validate_http_endpoint(field: &str, endpoint: &str) -> Result<(), ConfigLoadError> {
    let parsed = url::Url::parse(endpoint).map_err(|error| invalid_field(field, error))?;
    if !matches!(parsed.scheme(), "http" | "https")
        || parsed.host_str().is_none()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
    {
        return Err(ConfigLoadError::Validation(format!(
            "{field} must be an HTTP or HTTPS URL without credentials"
        )));
    }
    Ok(())
}

fn validate_proxy(proxy: Option<&str>) -> Result<(), ConfigLoadError> {
    if let Some(proxy) = proxy {
        fixer_http::HttpConfig::default()
            .with_proxy(proxy.to_owned())
            .map_err(|error| invalid_field("proxy", error))?;
    }
    Ok(())
}

fn validate_allowed_origins(origins: &[String]) -> Result<(), ConfigLoadError> {
    for origin in origins {
        let parsed = url::Url::parse(origin)
            .map_err(|error| invalid_field("server.allowed_origins", error))?;
        if !matches!(parsed.scheme(), "http" | "https")
            || !parsed.username().is_empty()
            || parsed.password().is_some()
            || parsed.host_str().is_none()
            || parsed.path() != "/"
            || parsed.query().is_some()
            || parsed.fragment().is_some()
            || origin.contains('*')
        {
            return Err(ConfigLoadError::Validation(
                "server.allowed_origins contains an invalid origin".to_owned(),
            ));
        }
    }
    Ok(())
}

fn validate_trusted_proxy(config: &TrustedProxyConfig) -> Result<(), ConfigLoadError> {
    let header = config
        .header
        .parse::<http::HeaderName>()
        .map_err(|error| invalid_field("server.trusted_proxy.header", error))?;
    if matches!(
        header,
        http::header::AUTHORIZATION | http::header::COOKIE | http::header::PROXY_AUTHORIZATION
    ) {
        return Err(ConfigLoadError::Validation(
            "server.trusted_proxy.header must not be a credential header".to_owned(),
        ));
    }
    for range in &config.ranges {
        range
            .parse::<ipnet::IpNet>()
            .map_err(|error| invalid_field("server.trusted_proxy.ranges", error))?;
    }
    Ok(())
}

fn invalid_field(field: &str, error: impl fmt::Display) -> ConfigLoadError {
    ConfigLoadError::Validation(format!("{field} is invalid: {error}"))
}

fn validate_secret_reference(name: &str, reference: Option<&str>) -> Result<(), ConfigLoadError> {
    if let Some(reference) = reference
        && (reference.is_empty()
            || !reference
                .bytes()
                .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_'))
    {
        return Err(ConfigLoadError::Validation(format!(
            "{name} must contain only uppercase ASCII letters, digits, and underscores"
        )));
    }
    Ok(())
}
fn resolve_reference(
    name: &str,
    reference: Option<&str>,
    environment: &BTreeMap<String, String>,
) -> Result<Option<SecretString>, ConfigLoadError> {
    validate_secret_reference(name, reference)?;
    reference
        .map(|reference| {
            environment
                .get(reference)
                .cloned()
                .map(SecretString::new)
                .ok_or_else(|| {
                    ConfigLoadError::Validation(format!(
                        "{name} references unset environment variable `{reference}`"
                    ))
                })
        })
        .transpose()
}
fn parse_bool(name: &str, value: &str) -> Result<bool, ConfigLoadError> {
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Ok(true),
        "0" | "false" | "no" | "off" => Ok(false),
        _ => Err(ConfigLoadError::Validation(format!(
            "{name} must be a boolean"
        ))),
    }
}
fn csv(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .collect()
}
fn temporary_path(path: &Path) -> PathBuf {
    static SEQUENCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(DEFAULT_CONFIG_FILE);
    let sequence = SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    path.with_file_name(format!(".{name}.{}.{}.tmp", std::process::id(), sequence))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{sync::mpsc, thread, time::Duration};

    #[test]
    fn snapshots_do_not_block_behind_filesystem_persistence() {
        let root = tempfile::tempdir().unwrap();
        let handle = ConfigLoader::new(root.path())
            .with_environment(BTreeMap::new())
            .load()
            .unwrap()
            .into_handle();
        let before = handle.snapshot();
        let mut next = before.clone();
        next.timeout_seconds = 52;
        let (started_tx, started_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let writer_handle = handle.clone();
        let writer = thread::spawn(move || {
            writer_handle.replace_after(next, |_| {
                started_tx.send(()).unwrap();
                release_rx.recv().unwrap();
                Ok(())
            })
        });
        started_rx.recv_timeout(Duration::from_secs(1)).unwrap();

        let (snapshot_tx, snapshot_rx) = mpsc::channel();
        let reader_handle = handle.clone();
        let reader = thread::spawn(move || snapshot_tx.send(reader_handle.snapshot()).unwrap());
        let observed = snapshot_rx.recv_timeout(Duration::from_secs(1));

        release_tx.send(()).unwrap();
        writer.join().unwrap().unwrap();
        reader.join().unwrap();
        assert_eq!(observed.unwrap(), before);
        assert_eq!(handle.snapshot().timeout_seconds, 52);
    }
}
