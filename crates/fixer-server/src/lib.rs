#![forbid(unsafe_code)]

pub mod api;
mod app;
pub mod auth;
mod fs_policy;
pub mod jobs;
mod network_policy;
mod observability;
pub mod store;
mod web;
mod workspace;

use std::{
    env, fmt,
    net::SocketAddr,
    num::NonZeroUsize,
    path::{Path, PathBuf},
};

pub use app::{app, job_app, secure_job_app, secure_workspace_app, workspace_app};
pub use auth::{AuthConfigError, AuthState, ClientIp};
pub use fs_policy::{FsPolicy, FsPolicyError};
pub use jobs::{JobFlowError, JobRuntime, SdkJobFlow, SearchSummary, WorkerPool};
pub use network_policy::{TrustedProxyError, TrustedProxyPolicy};
pub use observability::{TracingInitError, init_tracing};
pub use store::SqliteJobStore;
use thiserror::Error;
pub use web::web_app;
pub use workspace::{WorkspaceState, WorkspaceStateError};

const DEFAULT_BIND_ADDR: SocketAddr =
    SocketAddr::new(std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST), 3000);
const DEFAULT_DATABASE_PATH: &str = "fixer.sqlite3";
const DEFAULT_EVENT_CAPACITY: NonZeroUsize = NonZeroUsize::new(256).unwrap();
const DEFAULT_WORKER_COUNT: NonZeroUsize = NonZeroUsize::new(2).unwrap();

/// Validated network, authentication, filesystem, and persistence configuration.
#[derive(Clone, PartialEq, Eq)]
pub struct ServerConfig {
    bind_addr: SocketAddr,
    database_path: PathBuf,
    media_policy: Option<FsPolicy>,
    https_termination: bool,
    allowed_origins: Vec<String>,
    trusted_proxy_policy: TrustedProxyPolicy,
    worker_count: NonZeroUsize,
}

impl fmt::Debug for ServerConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ServerConfig")
            .field("bind_addr", &self.bind_addr)
            .field("database_path", &self.database_path)
            .field("media_policy", &self.media_policy)
            .field("https_termination", &self.https_termination)
            .field("allowed_origins", &self.allowed_origins)
            .field("trusted_proxy_policy", &self.trusted_proxy_policy)
            .field("worker_count", &self.worker_count)
            .finish()
    }
}

impl ServerConfig {
    /// Creates a configuration whose administrator credentials are stored in SQLite.
    pub fn new(bind_addr: SocketAddr) -> Result<Self, ServerConfigError> {
        Ok(Self::base(bind_addr))
    }

    /// Parses a server bind address.
    pub fn parse(value: &str) -> Result<Self, ServerConfigError> {
        Self::new(parse_bind(value)?)
    }

    /// Adapts the validated shared `fixer.toml` server subsection.
    pub fn from_shared(shared: &fixer_runtime::ServerConfig) -> Result<Self, ServerConfigError> {
        let worker_count =
            NonZeroUsize::new(shared.worker_count).ok_or(ServerConfigError::InvalidWorkerCount)?;
        let mut config = Self::new(shared.bind)?
            .with_database_path(shared.database.clone())?
            .with_https_termination(shared.https_termination)
            .with_allowed_origins(shared.allowed_origins.iter().map(String::as_str))?;
        if !shared.media_roots.is_empty() {
            config = config.with_media_roots(&shared.media_roots)?;
        }
        if !shared.trusted_proxy.ranges.is_empty() {
            config = config.with_trusted_proxy(
                shared.trusted_proxy.ranges.iter().map(String::as_str),
                &shared.trusted_proxy.header,
            )?;
        }
        config.worker_count = worker_count;
        Ok(config)
    }

    fn base(bind_addr: SocketAddr) -> Self {
        Self {
            bind_addr,
            database_path: PathBuf::from(DEFAULT_DATABASE_PATH),
            media_policy: None,
            https_termination: false,
            allowed_origins: Vec::new(),
            trusted_proxy_policy: TrustedProxyPolicy::disabled(),
            worker_count: DEFAULT_WORKER_COUNT,
        }
    }

    pub fn with_database_path(
        mut self,
        path: impl Into<PathBuf>,
    ) -> Result<Self, ServerConfigError> {
        let path = path.into();
        if path.as_os_str().is_empty() {
            return Err(ServerConfigError::EmptyDatabasePath);
        }
        self.database_path = path;
        Ok(self)
    }

    pub fn with_media_roots<I, P>(mut self, roots: I) -> Result<Self, ServerConfigError>
    where
        I: IntoIterator<Item = P>,
        P: AsRef<Path>,
    {
        self.media_policy = Some(FsPolicy::new(roots)?);
        Ok(self)
    }

    pub const fn with_https_termination(mut self, enabled: bool) -> Self {
        self.https_termination = enabled;
        self
    }

    pub fn with_allowed_origins<I, S>(mut self, origins: I) -> Result<Self, ServerConfigError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        self.allowed_origins = auth::normalize_origins(origins)?;
        Ok(self)
    }

    pub fn with_trusted_proxy<I, S>(
        mut self,
        ranges: I,
        client_ip_header: &str,
    ) -> Result<Self, ServerConfigError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        self.trusted_proxy_policy = TrustedProxyPolicy::new(ranges, client_ip_header)?;
        Ok(self)
    }

    /// Loads secure production settings from environment variables.
    pub fn from_env() -> Result<Self, ServerConfigError> {
        let bind = match env::var("FIXER_SERVER_BIND") {
            Ok(value) => parse_bind(&value)?,
            Err(env::VarError::NotPresent) => DEFAULT_BIND_ADDR,
            Err(source) => return Err(ServerConfigError::InvalidBindEnvironment(source)),
        };
        let mut config = Self::new(bind)?;
        if let Some(path) = optional_env("FIXER_SERVER_DATABASE")? {
            config = config.with_database_path(path)?;
        }
        if let Some(roots) = optional_env("FIXER_SERVER_MEDIA_ROOTS")? {
            config = config.with_media_roots(env::split_paths(&roots))?;
        }
        if let Some(value) = optional_env("FIXER_SERVER_HTTPS_TERMINATION")? {
            config = config
                .with_https_termination(parse_bool("FIXER_SERVER_HTTPS_TERMINATION", &value)?);
        }
        if let Some(origins) = optional_env("FIXER_SERVER_ALLOWED_ORIGINS")? {
            config = config.with_allowed_origins(csv_values(&origins))?;
        }
        let proxy_ranges = optional_env("FIXER_SERVER_TRUSTED_PROXY_RANGES")?;
        let proxy_header = optional_env("FIXER_SERVER_TRUSTED_PROXY_HEADER")?;
        match (proxy_ranges, proxy_header) {
            (None, None) => {}
            (Some(ranges), Some(header)) => {
                config = config.with_trusted_proxy(csv_values(&ranges), &header)?;
            }
            _ => return Err(ServerConfigError::IncompleteTrustedProxy),
        }
        Ok(config)
    }

    pub fn validate_for_serve(&self) -> Result<(), ServerConfigError> {
        if self.media_policy.is_none() {
            return Err(ServerConfigError::MissingMediaRoots);
        }
        Ok(())
    }

    pub const fn bind_addr(&self) -> SocketAddr {
        self.bind_addr
    }

    pub fn database_path(&self) -> &Path {
        &self.database_path
    }

    pub fn media_roots(&self) -> &[PathBuf] {
        self.media_policy
            .as_ref()
            .map_or(&[], |policy| policy.roots())
    }

    pub const fn https_termination(&self) -> bool {
        self.https_termination
    }

    pub fn allowed_origins(&self) -> &[String] {
        &self.allowed_origins
    }

    pub const fn trusted_proxy_policy(&self) -> &TrustedProxyPolicy {
        &self.trusted_proxy_policy
    }

    pub const fn worker_count(&self) -> NonZeroUsize {
        self.worker_count
    }
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self::base(DEFAULT_BIND_ADDR)
    }
}

#[derive(Debug, Error)]
pub enum ServerConfigError {
    #[error("at least one allowed media root is required")]
    MissingMediaRoots,
    #[error("invalid server bind address `{value}`: {source}")]
    InvalidBind {
        value: String,
        source: std::net::AddrParseError,
    },
    #[error("FIXER_SERVER_BIND is not valid Unicode: {0}")]
    InvalidBindEnvironment(env::VarError),
    #[error("FIXER_SERVER_DATABASE is not valid Unicode: {0}")]
    InvalidDatabaseEnvironment(env::VarError),
    #[error("{name} is not valid Unicode: {source}")]
    InvalidEnvironment {
        name: &'static str,
        source: env::VarError,
    },
    #[error("{name} must be `true` or `false`, found `{value}`")]
    InvalidBoolean { name: &'static str, value: String },
    #[error("SQLite database path must not be empty")]
    EmptyDatabasePath,
    #[error("trusted proxy ranges and header must be configured together")]
    IncompleteTrustedProxy,
    #[error("server worker_count must be greater than zero")]
    InvalidWorkerCount,
    #[error(transparent)]
    Authentication(#[from] AuthConfigError),
    #[error(transparent)]
    FilesystemPolicy(#[from] FsPolicyError),
    #[error(transparent)]
    TrustedProxy(#[from] TrustedProxyError),
}

#[derive(Debug, Error)]
pub enum ServeError {
    #[error("server configuration is invalid: {0}")]
    Config(#[from] ServerConfigError),
    #[error("failed to open persistent job store: {0}")]
    Store(#[from] store::StoreError),
    #[error("workspace initialization failed: {0}")]
    Workspace(#[from] WorkspaceStateError),
    #[error("server I/O failed: {0}")]
    Io(#[from] std::io::Error),
}

/// Opens persistent services with the legacy local-only worker flow.
pub async fn serve(config: ServerConfig, web_root: impl AsRef<Path>) -> Result<(), ServeError> {
    serve_inner(config, web_root, None).await
}

/// Opens persistent services using a fixed provider configuration.
pub async fn serve_configured(
    config: ServerConfig,
    web_root: impl AsRef<Path>,
    runtime_config: fixer_runtime::FixerConfig,
) -> Result<(), ServeError> {
    serve_inner(
        config,
        web_root,
        Some(RuntimeConfiguration::Static(Box::new(runtime_config))),
    )
    .await
}

/// Opens persistent services with one mutable configuration shared by routes and workers.
pub async fn serve_with_config_handle(
    config: ServerConfig,
    web_root: impl AsRef<Path>,
    runtime_config: fixer_runtime::ConfigHandle,
) -> Result<(), ServeError> {
    serve_inner(
        config,
        web_root,
        Some(RuntimeConfiguration::Shared(runtime_config)),
    )
    .await
}

enum RuntimeConfiguration {
    Static(Box<fixer_runtime::FixerConfig>),
    Shared(fixer_runtime::ConfigHandle),
}

async fn serve_inner(
    config: ServerConfig,
    web_root: impl AsRef<Path>,
    runtime_config: Option<RuntimeConfiguration>,
) -> Result<(), ServeError> {
    config.validate_for_serve()?;
    let store = SqliteJobStore::open(config.database_path()).await?;
    let fs_policy = config
        .media_policy
        .clone()
        .expect("validated production configuration has media roots");
    let workspace_state = match runtime_config.as_ref() {
        Some(RuntimeConfiguration::Shared(runtime_config)) => {
            WorkspaceState::new_with_config(fs_policy.roots(), runtime_config.clone())?
        }
        Some(RuntimeConfiguration::Static(_)) | None => WorkspaceState::new(fs_policy.roots())?,
    };
    let runtime = JobRuntime::new(store.clone(), DEFAULT_EVENT_CAPACITY).with_fs_policy(fs_policy);
    let auth_state = AuthState::new(store)
        .with_secure_cookie(config.https_termination)
        .with_allowed_origins(config.allowed_origins.iter().map(String::as_str))
        .map_err(ServerConfigError::from)?
        .with_trusted_proxy_policy(config.trusted_proxy_policy.clone());
    let listener = tokio::net::TcpListener::bind(config.bind_addr()).await?;
    tracing::info!(
        bind = %listener.local_addr()?,
        database = %config.database_path().display(),
        worker_count = config.worker_count().get(),
        "server listening"
    );
    let workers = match runtime_config {
        Some(RuntimeConfiguration::Static(runtime_config)) => runtime.start_workers(
            config.worker_count(),
            SdkJobFlow::from_config(*runtime_config),
        ),
        Some(RuntimeConfiguration::Shared(runtime_config)) => runtime.start_workers(
            config.worker_count(),
            SdkJobFlow::from_handle(runtime_config),
        ),
        None => runtime.start_local_workers(config.worker_count()),
    };
    let application = web_app(
        secure_workspace_app(runtime, auth_state, workspace_state),
        web_root,
    );
    let serve_result = axum::serve(
        listener,
        application.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await;
    workers.shutdown().await;
    serve_result?;
    Ok(())
}

fn parse_bind(value: &str) -> Result<SocketAddr, ServerConfigError> {
    value
        .parse()
        .map_err(|source| ServerConfigError::InvalidBind {
            value: value.to_owned(),
            source,
        })
}

fn optional_env(name: &'static str) -> Result<Option<String>, ServerConfigError> {
    match env::var(name) {
        Ok(value) => Ok(Some(value)),
        Err(env::VarError::NotPresent) => Ok(None),
        Err(source) if name == "FIXER_SERVER_DATABASE" => {
            Err(ServerConfigError::InvalidDatabaseEnvironment(source))
        }
        Err(source) => Err(ServerConfigError::InvalidEnvironment { name, source }),
    }
}

fn parse_bool(name: &'static str, value: &str) -> Result<bool, ServerConfigError> {
    match value {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err(ServerConfigError::InvalidBoolean {
            name,
            value: value.to_owned(),
        }),
    }
}

fn csv_values(value: &str) -> impl Iterator<Item = &str> {
    value
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
}
