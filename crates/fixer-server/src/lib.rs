#![forbid(unsafe_code)]

pub mod api;
mod app;
pub mod auth;
mod fs_policy;
pub mod jobs;
mod network_policy;
pub mod store;
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
pub use store::SqliteJobStore;
use thiserror::Error;
pub use workspace::{WorkspaceState, WorkspaceStateError};

const DEFAULT_BIND_ADDR: SocketAddr =
    SocketAddr::new(std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST), 3000);
const DEFAULT_DATABASE_PATH: &str = "fixer.sqlite3";
const DEFAULT_EVENT_CAPACITY: NonZeroUsize = NonZeroUsize::new(256).unwrap();
const DEFAULT_WORKER_COUNT: NonZeroUsize = NonZeroUsize::new(2).unwrap();
const MAX_PASSWORD_BYTES: usize = 1024;

#[derive(Clone, PartialEq, Eq)]
struct ServerPassword(String);

impl fmt::Debug for ServerPassword {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("[REDACTED]")
    }
}

/// Validated network, authentication, filesystem, and persistence configuration.
#[derive(Clone, PartialEq, Eq)]
pub struct ServerConfig {
    bind_addr: SocketAddr,
    database_path: PathBuf,
    password: Option<ServerPassword>,
    media_policy: Option<FsPolicy>,
    https_termination: bool,
    allowed_origins: Vec<String>,
    trusted_proxy_policy: TrustedProxyPolicy,
}

impl fmt::Debug for ServerConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ServerConfig")
            .field("bind_addr", &self.bind_addr)
            .field("database_path", &self.database_path)
            .field("password", &self.password)
            .field("media_policy", &self.media_policy)
            .field("https_termination", &self.https_termination)
            .field("allowed_origins", &self.allowed_origins)
            .field("trusted_proxy_policy", &self.trusted_proxy_policy)
            .finish()
    }
}

impl ServerConfig {
    /// Creates an unauthenticated loopback-only configuration for validation and tests.
    pub fn new(bind_addr: SocketAddr) -> Result<Self, ServerConfigError> {
        if !bind_addr.ip().is_loopback() {
            return Err(ServerConfigError::AuthenticationRequired);
        }
        Ok(Self::base(bind_addr))
    }

    /// Creates a configuration with the single-user password required for any bind address.
    pub fn authenticated(
        bind_addr: SocketAddr,
        password: impl Into<String>,
    ) -> Result<Self, ServerConfigError> {
        let mut config = Self::base(bind_addr);
        config.password = Some(validate_password(password.into())?);
        Ok(config)
    }

    /// Parses an unauthenticated loopback bind address.
    pub fn parse(value: &str) -> Result<Self, ServerConfigError> {
        Self::new(parse_bind(value)?)
    }

    fn base(bind_addr: SocketAddr) -> Self {
        Self {
            bind_addr,
            database_path: PathBuf::from(DEFAULT_DATABASE_PATH),
            password: None,
            media_policy: None,
            https_termination: false,
            allowed_origins: Vec::new(),
            trusted_proxy_policy: TrustedProxyPolicy::disabled(),
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
        let mut config = match env::var("FIXER_SERVER_PASSWORD") {
            Ok(password) => Self::authenticated(bind, password)?,
            Err(env::VarError::NotPresent) => {
                if bind.ip().is_loopback() {
                    Self::new(bind)?
                } else {
                    return Err(ServerConfigError::AuthenticationRequired);
                }
            }
            Err(source) => {
                return Err(ServerConfigError::InvalidEnvironment {
                    name: "FIXER_SERVER_PASSWORD",
                    source,
                });
            }
        };
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
        if self.password.is_none() {
            return Err(ServerConfigError::MissingPassword);
        }
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
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self::base(DEFAULT_BIND_ADDR)
    }
}

#[derive(Debug, Error)]
pub enum ServerConfigError {
    #[error("non-loopback binding requires authentication")]
    AuthenticationRequired,
    #[error("server authentication password is required")]
    MissingPassword,
    #[error("server authentication password must contain between 1 and {MAX_PASSWORD_BYTES} bytes")]
    InvalidPassword,
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
    #[error("password hashing failed: {0}")]
    Password(#[from] auth::password::PasswordError),
    #[error("password hashing worker failed: {0}")]
    PasswordTask(#[from] tokio::task::JoinError),
    #[error("workspace initialization failed: {0}")]
    Workspace(#[from] WorkspaceStateError),
    #[error("server I/O failed: {0}")]
    Io(#[from] std::io::Error),
}

/// Opens persistent services, installs every security boundary, binds, and serves.
pub async fn serve(config: ServerConfig) -> Result<(), ServeError> {
    config.validate_for_serve()?;
    let store = SqliteJobStore::open(config.database_path()).await?;
    let password = config
        .password
        .as_ref()
        .expect("validated production configuration has a password")
        .0
        .clone();
    let password_hash =
        tokio::task::spawn_blocking(move || auth::password::hash_password(&password)).await??;
    store.set_password_hash(&password_hash).await?;

    let fs_policy = config
        .media_policy
        .clone()
        .expect("validated production configuration has media roots");
    let workspace_state = WorkspaceState::new(fs_policy.roots())?;
    let runtime = JobRuntime::new(store.clone(), DEFAULT_EVENT_CAPACITY).with_fs_policy(fs_policy);
    let auth_state = AuthState::new(store)
        .with_secure_cookie(config.https_termination)
        .with_allowed_origins(config.allowed_origins.iter().map(String::as_str))
        .map_err(ServerConfigError::from)?
        .with_trusted_proxy_policy(config.trusted_proxy_policy.clone());
    let listener = tokio::net::TcpListener::bind(config.bind_addr()).await?;
    let workers = runtime.start_local_workers(DEFAULT_WORKER_COUNT);
    let application = secure_workspace_app(runtime, auth_state, workspace_state);
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

fn validate_password(password: String) -> Result<ServerPassword, ServerConfigError> {
    if password.is_empty() || password.len() > MAX_PASSWORD_BYTES {
        return Err(ServerConfigError::InvalidPassword);
    }
    Ok(ServerPassword(password))
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
