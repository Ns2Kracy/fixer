use std::{
    collections::{HashSet, VecDeque},
    fmt, fs,
    path::{Component, Path, PathBuf},
    sync::{Arc, RwLock},
    time::Duration,
};

use fixer_core::{Header, HttpClient, HttpError, HttpMethod, HttpRequest};
use fixer_http::{HttpConfig, ReqwestHttpClient};
use fixer_runtime::{
    ConfigHandle, ConfigWriteError, ConflictPolicy, FixerConfig, OutputPreset, PlacementPolicy,
    SecretString,
};
use language_tags::LanguageTag;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use url::Url;

const KNOWN_PROVIDERS: &[&str] = &[
    "local",
    "tmdb",
    "bangumi",
    "anilist",
    "musicbrainz",
    "openlibrary",
];
const MAX_LIBRARY_ENTRIES: usize = 100;
const MAX_SEARCH_LIMIT: usize = 100;
const MAX_SEARCH_VISITS: usize = 10_000;
const MAX_RELATIVE_PATH_BYTES: usize = 4096;
const MAX_SECRET_BYTES: usize = 4096;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ProviderEndpoints {
    pub(crate) tmdb: String,
    pub(crate) bangumi: String,
    pub(crate) anilist: String,
    pub(crate) musicbrainz: String,
    pub(crate) openlibrary: String,
    pub(crate) openlibrary_cover: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkspaceSettingsInput {
    pub(crate) offline: bool,
    pub(crate) proxy: Option<String>,
    pub(crate) preferred_locales: Vec<String>,
    pub(crate) timeout_seconds: u64,
    pub(crate) auto_accept_confidence: f32,
    pub(crate) review_confidence: f32,
    pub(crate) output_preset: OutputPreset,
    pub(crate) placement: PlacementPolicy,
    pub(crate) conflict_policy: ConflictPolicy,
    pub(crate) enabled_providers: Vec<String>,
    pub(crate) provider_endpoints: ProviderEndpoints,
    pub(crate) tmdb_api_token: Option<String>,
    pub(crate) anilist_access_token: Option<String>,
    #[serde(default)]
    pub(crate) clear_tmdb_api_token: bool,
    #[serde(default)]
    pub(crate) clear_anilist_access_token: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub(crate) struct WorkspaceSettingsSnapshot {
    pub(crate) offline: bool,
    pub(crate) proxy: Option<String>,
    pub(crate) preferred_locales: Vec<String>,
    pub(crate) timeout_seconds: u64,
    pub(crate) auto_accept_confidence: f32,
    pub(crate) review_confidence: f32,
    pub(crate) output_preset: OutputPreset,
    pub(crate) placement: PlacementPolicy,
    pub(crate) conflict_policy: ConflictPolicy,
    pub(crate) enabled_providers: Vec<String>,
    pub(crate) provider_endpoints: ProviderEndpoints,
    pub(crate) secrets: SecretStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct SecretStatus {
    pub(crate) tmdb_api_token_configured: bool,
    pub(crate) anilist_access_token_configured: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) tmdb_api_token_env: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) anilist_access_token_env: Option<String>,
}

impl WorkspaceSettingsSnapshot {
    fn from_config(config: &FixerConfig) -> Self {
        Self {
            offline: config.offline,
            proxy: config.proxy.clone(),
            preferred_locales: config.preferred_locales.clone(),
            timeout_seconds: config.timeout_seconds,
            auto_accept_confidence: config.auto_accept_confidence,
            review_confidence: config.review_confidence,
            output_preset: config.output_preset,
            placement: config.placement,
            conflict_policy: config.conflict_policy,
            enabled_providers: config.enabled_providers.clone(),
            provider_endpoints: ProviderEndpoints {
                tmdb: config.providers.tmdb.base_url.clone(),
                bangumi: config.providers.bangumi.base_url.clone(),
                anilist: config.providers.anilist.base_url.clone(),
                musicbrainz: config.providers.musicbrainz.base_url.clone(),
                openlibrary: config.providers.openlibrary.base_url.clone(),
                openlibrary_cover: config.providers.openlibrary.cover_base_url.clone(),
            },
            secrets: SecretStatus {
                tmdb_api_token_configured: config.providers.tmdb.resolved_api_token().is_some(),
                anilist_access_token_configured: config
                    .providers
                    .anilist
                    .resolved_access_token()
                    .is_some(),
                tmdb_api_token_env: config.providers.tmdb.api_token_env.clone(),
                anilist_access_token_env: config.providers.anilist.access_token_env.clone(),
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct RootSummary {
    pub(crate) id: String,
    pub(crate) label: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum LibraryEntryKind {
    Directory,
    File,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct LibraryEntry {
    pub(crate) name: String,
    pub(crate) path: String,
    pub(crate) kind: LibraryEntryKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) size_bytes: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LibraryListing {
    pub(crate) root_id: String,
    pub(crate) path: String,
    pub(crate) entries: Vec<LibraryEntry>,
    pub(crate) truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct SearchMatch {
    pub(crate) root_id: String,
    pub(crate) path: String,
    pub(crate) name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SearchMatches {
    pub(crate) results: Vec<SearchMatch>,
    pub(crate) truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct ProviderProbeResult {
    pub(crate) provider: String,
    pub(crate) ok: bool,
    pub(crate) category: &'static str,
    pub(crate) message: &'static str,
}

#[derive(Clone)]
struct WorkspaceRoot {
    id: String,
    label: String,
    path: PathBuf,
}

#[derive(Clone)]
enum WorkspaceConfig {
    Shared(ConfigHandle),
    Ephemeral(Arc<RwLock<FixerConfig>>),
}

impl WorkspaceConfig {
    fn snapshot(&self) -> FixerConfig {
        match self {
            Self::Shared(handle) => handle.snapshot(),
            Self::Ephemeral(config) => config
                .read()
                .expect("workspace configuration lock is not poisoned")
                .clone(),
        }
    }

    async fn replace(&self, next: FixerConfig) -> Result<(), WorkspaceStateError> {
        match self {
            Self::Shared(handle) => {
                let handle = handle.clone();
                tokio::task::spawn_blocking(move || handle.replace_and_persist(next))
                    .await
                    .map_err(|_| WorkspaceStateError::SettingsTask)?
                    .map_err(WorkspaceStateError::SettingsPersistence)
            }
            Self::Ephemeral(config) => {
                next.validate().map_err(|error| {
                    WorkspaceStateError::SettingsPersistence(ConfigWriteError::Validation(error))
                })?;
                *config
                    .write()
                    .expect("workspace configuration lock is not poisoned") = next;
                Ok(())
            }
        }
    }
}

struct WorkspaceInner {
    roots: Vec<WorkspaceRoot>,
    config: WorkspaceConfig,
}

#[derive(Clone)]
pub struct WorkspaceState {
    inner: Arc<WorkspaceInner>,
}

impl fmt::Debug for WorkspaceState {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("WorkspaceState")
            .field("root_count", &self.inner.roots.len())
            .finish_non_exhaustive()
    }
}

impl Default for WorkspaceState {
    fn default() -> Self {
        Self {
            inner: Arc::new(WorkspaceInner {
                roots: Vec::new(),
                config: WorkspaceConfig::Ephemeral(Arc::new(RwLock::new(FixerConfig::default()))),
            }),
        }
    }
}

impl WorkspaceState {
    pub fn new<I, P>(roots: I) -> Result<Self, WorkspaceStateError>
    where
        I: IntoIterator<Item = P>,
        P: AsRef<Path>,
    {
        Self::build(
            roots,
            WorkspaceConfig::Ephemeral(Arc::new(RwLock::new(FixerConfig::default()))),
        )
    }

    pub fn new_with_config<I, P>(
        roots: I,
        config: ConfigHandle,
    ) -> Result<Self, WorkspaceStateError>
    where
        I: IntoIterator<Item = P>,
        P: AsRef<Path>,
    {
        Self::build(roots, WorkspaceConfig::Shared(config))
    }

    fn build<I, P>(roots: I, config: WorkspaceConfig) -> Result<Self, WorkspaceStateError>
    where
        I: IntoIterator<Item = P>,
        P: AsRef<Path>,
    {
        let mut canonical_roots = Vec::new();
        for root in roots {
            let path = root
                .as_ref()
                .canonicalize()
                .map_err(|_| WorkspaceStateError::InvalidRoot)?;
            if !path.is_dir() {
                return Err(WorkspaceStateError::InvalidRoot);
            }
            if canonical_roots
                .iter()
                .any(|existing: &WorkspaceRoot| existing.path == path)
            {
                continue;
            }
            let label = path
                .file_name()
                .and_then(|value| value.to_str())
                .filter(|value| !value.is_empty())
                .unwrap_or("Media root")
                .to_owned();
            canonical_roots.push(WorkspaceRoot {
                id: format!("root-{}", canonical_roots.len()),
                label,
                path,
            });
        }
        Ok(Self {
            inner: Arc::new(WorkspaceInner {
                roots: canonical_roots,
                config,
            }),
        })
    }

    pub(crate) async fn settings(&self) -> WorkspaceSettingsSnapshot {
        WorkspaceSettingsSnapshot::from_config(&self.inner.config.snapshot())
    }

    pub(crate) async fn update_settings(
        &self,
        input: WorkspaceSettingsInput,
    ) -> Result<WorkspaceSettingsSnapshot, WorkspaceStateError> {
        validate_settings(&input)?;
        let mut next = self.inner.config.snapshot();
        apply_settings_input(&mut next, input);
        self.inner.config.replace(next).await?;
        Ok(WorkspaceSettingsSnapshot::from_config(
            &self.inner.config.snapshot(),
        ))
    }

    pub(crate) fn roots(&self) -> Vec<RootSummary> {
        self.inner
            .roots
            .iter()
            .map(|root| RootSummary {
                id: root.id.clone(),
                label: root.label.clone(),
            })
            .collect()
    }

    pub(crate) fn list(
        &self,
        root_id: &str,
        relative: &str,
    ) -> Result<LibraryListing, WorkspaceStateError> {
        let root = self.root(root_id)?;
        let directory = resolve_relative(root, relative)?;
        if !directory.is_dir() {
            return Err(WorkspaceStateError::NotDirectory);
        }
        let mut entries = fs::read_dir(&directory)
            .map_err(|_| WorkspaceStateError::Inspect)?
            .filter_map(Result::ok)
            .filter_map(|entry| library_entry(root, entry).transpose())
            .collect::<Result<Vec<_>, _>>()?;
        entries.sort_by(|left, right| {
            matches!(left.kind, LibraryEntryKind::File)
                .cmp(&matches!(right.kind, LibraryEntryKind::File))
                .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
        });
        let truncated = entries.len() > MAX_LIBRARY_ENTRIES;
        entries.truncate(MAX_LIBRARY_ENTRIES);
        Ok(LibraryListing {
            root_id: root.id.clone(),
            path: normalize_display_path(relative),
            entries,
            truncated,
        })
    }

    pub(crate) fn search(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<SearchMatches, WorkspaceStateError> {
        let query = query.trim();
        if query.is_empty() || query.len() > 200 {
            return Err(WorkspaceStateError::InvalidInput {
                field: "query",
                reason: "must contain 1 to 200 characters",
            });
        }
        if !(1..=MAX_SEARCH_LIMIT).contains(&limit) {
            return Err(WorkspaceStateError::InvalidInput {
                field: "limit",
                reason: "must be between 1 and 100",
            });
        }
        let needle = query.to_lowercase();
        let mut matches = Vec::new();
        let mut visits = 0usize;
        for root in &self.inner.roots {
            let mut queue = VecDeque::from([root.path.clone()]);
            while let Some(directory) = queue.pop_front() {
                let Ok(entries) = fs::read_dir(directory) else {
                    continue;
                };
                for entry in entries.flatten() {
                    visits = visits.saturating_add(1);
                    if visits > MAX_SEARCH_VISITS {
                        return Ok(SearchMatches {
                            results: matches,
                            truncated: true,
                        });
                    }
                    let Ok(file_type) = entry.file_type() else {
                        continue;
                    };
                    if file_type.is_symlink() {
                        continue;
                    }
                    let path = entry.path();
                    if file_type.is_dir() {
                        queue.push_back(path);
                        continue;
                    }
                    if !file_type.is_file() {
                        continue;
                    }
                    let name = entry.file_name().to_string_lossy().into_owned();
                    if !name.to_lowercase().contains(&needle) {
                        continue;
                    }
                    matches.push(SearchMatch {
                        root_id: root.id.clone(),
                        path: relative_display(&root.path, &path)?,
                        name,
                    });
                    if matches.len() > limit {
                        matches.truncate(limit);
                        return Ok(SearchMatches {
                            results: matches,
                            truncated: true,
                        });
                    }
                }
            }
        }
        Ok(SearchMatches {
            results: matches,
            truncated: false,
        })
    }

    pub(crate) async fn probe_provider(
        &self,
        provider: &str,
    ) -> Result<ProviderProbeResult, WorkspaceStateError> {
        if !KNOWN_PROVIDERS.contains(&provider) {
            return Err(WorkspaceStateError::ProviderNotFound);
        }
        let settings = self.inner.config.snapshot();
        if !settings
            .enabled_providers
            .iter()
            .any(|enabled| enabled == provider)
        {
            return Ok(probe(provider, false, "disabled", "Provider is disabled"));
        }
        if provider == "local" {
            return Ok(probe(provider, true, "ready", "Local provider is ready"));
        }
        if settings.offline {
            return Ok(probe(
                provider,
                false,
                "offline",
                "Offline mode skips network providers",
            ));
        }
        if provider == "tmdb" && settings.providers.tmdb.resolved_api_token().is_none() {
            return Ok(probe(
                provider,
                false,
                "credentials_missing",
                "Provider credentials are not configured",
            ));
        }
        let endpoint =
            endpoint(&settings, provider).ok_or(WorkspaceStateError::ProviderNotFound)?;
        let mut config =
            HttpConfig::default().with_timeout(Duration::from_secs(settings.timeout_seconds));
        if let Some(proxy) = &settings.proxy {
            config = config
                .with_proxy(proxy.clone())
                .map_err(|_| WorkspaceStateError::ProbeConfiguration)?;
        }
        let client =
            ReqwestHttpClient::new(config).map_err(|_| WorkspaceStateError::ProbeConfiguration)?;
        let mut request = HttpRequest::new(HttpMethod::Head, endpoint);
        if provider == "tmdb" {
            let token = settings
                .providers
                .tmdb
                .resolved_api_token()
                .expect("TMDB credentials were checked above");
            request = request.with_header(
                Header::new("authorization", format!("Bearer {token}"))
                    .map_err(|_| WorkspaceStateError::ProbeConfiguration)?,
            );
        } else if provider == "anilist"
            && let Some(token) = settings.providers.anilist.resolved_access_token()
        {
            request = request.with_header(
                Header::new("authorization", format!("Bearer {token}"))
                    .map_err(|_| WorkspaceStateError::ProbeConfiguration)?,
            );
        }
        let result = client.execute(request).await;
        Ok(match result {
            Ok(_) => probe(provider, true, "ready", "Provider endpoint is reachable"),
            Err(HttpError::Status { status: 401 | 403 }) => probe(
                provider,
                false,
                "credentials_rejected",
                "Provider rejected the configured credentials",
            ),
            Err(HttpError::Status { status: 429 }) => probe(
                provider,
                false,
                "rate_limited",
                "Provider rate limit prevented the test",
            ),
            Err(HttpError::Status { status: 500..=599 }) => probe(
                provider,
                false,
                "upstream_error",
                "Provider returned a temporary server error",
            ),
            Err(HttpError::Status { .. }) => {
                probe(provider, true, "ready", "Provider endpoint is reachable")
            }
            Err(HttpError::Timeout) => probe(
                provider,
                false,
                "timeout",
                "Provider connectivity test timed out",
            ),
            Err(HttpError::Transport(_)) => probe(
                provider,
                false,
                "unreachable",
                "Provider endpoint could not be reached",
            ),
            Err(HttpError::InvalidMessage(_) | HttpError::Offline) => probe(
                provider,
                false,
                "configuration_error",
                "Provider connectivity test could not be configured",
            ),
            Err(_) => probe(
                provider,
                false,
                "configuration_error",
                "Provider connectivity test could not be configured",
            ),
        })
    }

    fn root(&self, id: &str) -> Result<&WorkspaceRoot, WorkspaceStateError> {
        self.inner
            .roots
            .iter()
            .find(|root| root.id == id)
            .ok_or(WorkspaceStateError::RootNotFound)
    }
}

fn apply_settings_input(config: &mut FixerConfig, input: WorkspaceSettingsInput) {
    config.offline = input.offline;
    config.proxy = input.proxy;
    config.preferred_locales = input.preferred_locales;
    config.timeout_seconds = input.timeout_seconds;
    config.auto_accept_confidence = input.auto_accept_confidence;
    config.review_confidence = input.review_confidence;
    config.output_preset = input.output_preset;
    config.placement = input.placement;
    config.conflict_policy = input.conflict_policy;
    config.enabled_providers = deduplicate(input.enabled_providers);
    config.providers.tmdb.base_url = input.provider_endpoints.tmdb;
    config.providers.bangumi.base_url = input.provider_endpoints.bangumi;
    config.providers.anilist.base_url = input.provider_endpoints.anilist;
    config.providers.musicbrainz.base_url = input.provider_endpoints.musicbrainz;
    config.providers.openlibrary.base_url = input.provider_endpoints.openlibrary;
    config.providers.openlibrary.cover_base_url = input.provider_endpoints.openlibrary_cover;
    update_secret(
        &mut config.providers.tmdb.api_token,
        &mut config.providers.tmdb.api_token_env,
        input.tmdb_api_token,
        input.clear_tmdb_api_token,
    );
    update_secret(
        &mut config.providers.anilist.access_token,
        &mut config.providers.anilist.access_token_env,
        input.anilist_access_token,
        input.clear_anilist_access_token,
    );
}

fn validate_settings(input: &WorkspaceSettingsInput) -> Result<(), WorkspaceStateError> {
    if input.preferred_locales.is_empty() {
        return Err(invalid("preferred_locales", "must not be empty"));
    }
    for locale in &input.preferred_locales {
        locale
            .parse::<LanguageTag>()
            .map_err(|_| invalid("preferred_locales", "must contain valid BCP 47 tags"))?;
    }
    if input.timeout_seconds == 0 || input.timeout_seconds > 300 {
        return Err(invalid("timeout_seconds", "must be between 1 and 300"));
    }
    if !valid_confidence(input.auto_accept_confidence) {
        return Err(invalid("auto_accept_confidence", "must be between 0 and 1"));
    }
    if !valid_confidence(input.review_confidence) {
        return Err(invalid("review_confidence", "must be between 0 and 1"));
    }
    if input.review_confidence > input.auto_accept_confidence {
        return Err(invalid(
            "review_confidence",
            "must not exceed auto_accept_confidence",
        ));
    }
    if let Some(proxy) = &input.proxy {
        let url = Url::parse(proxy)
            .map_err(|_| invalid("proxy", "must be a valid HTTP, HTTPS, or SOCKS proxy URL"))?;
        if !url.username().is_empty() || url.password().is_some() {
            return Err(invalid("proxy", "must not contain credentials"));
        }
        HttpConfig::default()
            .with_proxy(proxy.clone())
            .map_err(|_| invalid("proxy", "must be a valid HTTP, HTTPS, or SOCKS proxy URL"))?;
    }
    if input.enabled_providers.is_empty() {
        return Err(invalid("enabled_providers", "must not be empty"));
    }
    if let Some(provider) = input
        .enabled_providers
        .iter()
        .find(|provider| !KNOWN_PROVIDERS.contains(&provider.as_str()))
    {
        return Err(WorkspaceStateError::InvalidInput {
            field: "enabled_providers",
            reason: if provider.is_empty() {
                "must contain known provider IDs"
            } else {
                "contains an unknown provider ID"
            },
        });
    }
    for (field, endpoint) in [
        (
            "provider_endpoints.tmdb",
            input.provider_endpoints.tmdb.as_str(),
        ),
        (
            "provider_endpoints.bangumi",
            input.provider_endpoints.bangumi.as_str(),
        ),
        (
            "provider_endpoints.anilist",
            input.provider_endpoints.anilist.as_str(),
        ),
        (
            "provider_endpoints.musicbrainz",
            input.provider_endpoints.musicbrainz.as_str(),
        ),
        (
            "provider_endpoints.openlibrary",
            input.provider_endpoints.openlibrary.as_str(),
        ),
        (
            "provider_endpoints.openlibrary_cover",
            input.provider_endpoints.openlibrary_cover.as_str(),
        ),
    ] {
        validate_endpoint(field, endpoint)?;
    }
    if !input.provider_endpoints.openlibrary_cover.ends_with('/') {
        return Err(invalid(
            "provider_endpoints.openlibrary_cover",
            "must end with a slash",
        ));
    }
    validate_secret_update(
        "tmdb_api_token",
        input.tmdb_api_token.as_deref(),
        input.clear_tmdb_api_token,
    )?;
    validate_secret_update(
        "anilist_access_token",
        input.anilist_access_token.as_deref(),
        input.clear_anilist_access_token,
    )?;
    Ok(())
}

fn validate_endpoint(field: &'static str, value: &str) -> Result<(), WorkspaceStateError> {
    let url =
        Url::parse(value).map_err(|_| invalid(field, "must be an absolute HTTP or HTTPS URL"))?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err(invalid(field, "must be an absolute HTTP or HTTPS URL"));
    }
    Ok(())
}

fn validate_secret_update(
    field: &'static str,
    value: Option<&str>,
    clear: bool,
) -> Result<(), WorkspaceStateError> {
    if clear && value.is_some() {
        return Err(invalid(field, "cannot be replaced and cleared together"));
    }
    if let Some(value) = value {
        if value.is_empty() || value.len() > MAX_SECRET_BYTES || value.chars().any(char::is_control)
        {
            return Err(invalid(
                field,
                "must contain 1 to 4096 non-control characters",
            ));
        }
    }
    Ok(())
}

fn update_secret(
    current: &mut Option<SecretString>,
    reference: &mut Option<String>,
    value: Option<String>,
    clear: bool,
) {
    if clear {
        *current = None;
        *reference = None;
    } else if let Some(value) = value {
        *current = Some(SecretString::new(value));
        *reference = None;
    }
}

fn valid_confidence(value: f32) -> bool {
    value.is_finite() && (0.0..=1.0).contains(&value)
}

fn deduplicate(values: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    values
        .into_iter()
        .filter(|value| seen.insert(value.clone()))
        .collect()
}

fn invalid(field: &'static str, reason: &'static str) -> WorkspaceStateError {
    WorkspaceStateError::InvalidInput { field, reason }
}

fn resolve_relative(root: &WorkspaceRoot, relative: &str) -> Result<PathBuf, WorkspaceStateError> {
    if relative.len() > MAX_RELATIVE_PATH_BYTES || relative.contains('\\') {
        return Err(WorkspaceStateError::InvalidPath);
    }
    let relative_path = Path::new(relative);
    if relative_path.is_absolute()
        || relative_path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
            && !relative.is_empty()
    {
        return Err(WorkspaceStateError::InvalidPath);
    }
    let candidate = if relative.is_empty() {
        root.path.clone()
    } else {
        root.path.join(relative_path)
    };
    let canonical = candidate
        .canonicalize()
        .map_err(|_| WorkspaceStateError::PathNotFound)?;
    if !canonical.starts_with(&root.path) {
        return Err(WorkspaceStateError::InvalidPath);
    }
    Ok(canonical)
}

fn library_entry(
    root: &WorkspaceRoot,
    entry: fs::DirEntry,
) -> Result<Option<LibraryEntry>, WorkspaceStateError> {
    let file_type = entry
        .file_type()
        .map_err(|_| WorkspaceStateError::Inspect)?;
    if file_type.is_symlink() || (!file_type.is_dir() && !file_type.is_file()) {
        return Ok(None);
    }
    let path = entry.path();
    let canonical = path
        .canonicalize()
        .map_err(|_| WorkspaceStateError::Inspect)?;
    if !canonical.starts_with(&root.path) {
        return Ok(None);
    }
    let name = entry.file_name().to_string_lossy().into_owned();
    let kind = if file_type.is_dir() {
        LibraryEntryKind::Directory
    } else {
        LibraryEntryKind::File
    };
    let size_bytes = if file_type.is_file() {
        entry.metadata().ok().map(|metadata| metadata.len())
    } else {
        None
    };
    Ok(Some(LibraryEntry {
        name,
        path: relative_display(&root.path, &canonical)?,
        kind,
        size_bytes,
    }))
}

fn relative_display(root: &Path, path: &Path) -> Result<String, WorkspaceStateError> {
    let relative = path
        .strip_prefix(root)
        .map_err(|_| WorkspaceStateError::InvalidPath)?;
    Ok(relative
        .components()
        .filter_map(|component| match component {
            Component::Normal(value) => Some(value.to_string_lossy()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/"))
}

fn normalize_display_path(path: &str) -> String {
    path.trim_matches('/').to_owned()
}

fn endpoint<'a>(config: &'a FixerConfig, provider: &str) -> Option<&'a str> {
    match provider {
        "tmdb" => Some(&config.providers.tmdb.base_url),
        "bangumi" => Some(&config.providers.bangumi.base_url),
        "anilist" => Some(&config.providers.anilist.base_url),
        "musicbrainz" => Some(&config.providers.musicbrainz.base_url),
        "openlibrary" => Some(&config.providers.openlibrary.base_url),
        _ => None,
    }
}

fn probe(
    provider: &str,
    ok: bool,
    category: &'static str,
    message: &'static str,
) -> ProviderProbeResult {
    ProviderProbeResult {
        provider: provider.to_owned(),
        ok,
        category,
        message,
    }
}

#[derive(Debug, Error)]
pub enum WorkspaceStateError {
    #[error("workspace media root is invalid")]
    InvalidRoot,
    #[error("request field is invalid")]
    InvalidInput {
        field: &'static str,
        reason: &'static str,
    },
    #[error("library root was not found")]
    RootNotFound,
    #[error("library path must be a safe relative path")]
    InvalidPath,
    #[error("library path was not found")]
    PathNotFound,
    #[error("library path is not a directory")]
    NotDirectory,
    #[error("library directory could not be inspected")]
    Inspect,
    #[error("provider was not found")]
    ProviderNotFound,
    #[error("provider probe could not be configured")]
    ProbeConfiguration,
    #[error("settings persistence task failed")]
    SettingsTask,
    #[error("settings could not be persisted")]
    SettingsPersistence(#[source] ConfigWriteError),
}
