#![forbid(unsafe_code)]

pub mod api;
mod app;
pub mod auth;
mod fs_policy;
pub mod jobs;
pub mod store;

use std::{
    env,
    net::SocketAddr,
    num::NonZeroUsize,
    path::{Path, PathBuf},
};

pub use app::{app, job_app};
pub use fs_policy::{FsPolicy, FsPolicyError};
pub use jobs::{JobFlowError, JobRuntime, SdkJobFlow, SearchSummary, WorkerPool};
pub use store::SqliteJobStore;
use thiserror::Error;

const DEFAULT_BIND_ADDR: SocketAddr =
    SocketAddr::new(std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST), 3000);
const DEFAULT_DATABASE_PATH: &str = "fixer.sqlite3";
const DEFAULT_EVENT_CAPACITY: NonZeroUsize = NonZeroUsize::new(256).unwrap();
const DEFAULT_WORKER_COUNT: NonZeroUsize = NonZeroUsize::new(2).unwrap();

/// Validated network and persistence configuration for the HTTP service.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerConfig {
    bind_addr: SocketAddr,
    database_path: PathBuf,
}

impl ServerConfig {
    /// Validates an explicit bind address and uses the default database path.
    ///
    /// Authentication is not implemented yet, so this server version accepts
    /// only loopback listeners.
    pub fn new(bind_addr: SocketAddr) -> Result<Self, ServerConfigError> {
        if !bind_addr.ip().is_loopback() {
            return Err(ServerConfigError::AuthenticationRequired);
        }
        Ok(Self {
            bind_addr,
            database_path: PathBuf::from(DEFAULT_DATABASE_PATH),
        })
    }

    /// Parses and validates a bind address before listener creation.
    pub fn parse(value: &str) -> Result<Self, ServerConfigError> {
        let bind_addr = value
            .parse()
            .map_err(|source| ServerConfigError::InvalidBind {
                value: value.to_owned(),
                source,
            })?;
        Self::new(bind_addr)
    }

    /// Overrides the SQLite database path used during startup.
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

    /// Loads `FIXER_SERVER_BIND` and `FIXER_SERVER_DATABASE`.
    pub fn from_env() -> Result<Self, ServerConfigError> {
        let mut config = match env::var("FIXER_SERVER_BIND") {
            Ok(value) => Self::parse(&value)?,
            Err(env::VarError::NotPresent) => Self::default(),
            Err(source) => return Err(ServerConfigError::InvalidBindEnvironment(source)),
        };
        match env::var("FIXER_SERVER_DATABASE") {
            Ok(path) => config = config.with_database_path(path)?,
            Err(env::VarError::NotPresent) => {}
            Err(source) => return Err(ServerConfigError::InvalidDatabaseEnvironment(source)),
        }
        Ok(config)
    }

    /// Returns the validated listener address.
    pub const fn bind_addr(&self) -> SocketAddr {
        self.bind_addr
    }

    /// Returns the SQLite database path opened before listener creation.
    pub fn database_path(&self) -> &Path {
        &self.database_path
    }
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            bind_addr: DEFAULT_BIND_ADDR,
            database_path: PathBuf::from(DEFAULT_DATABASE_PATH),
        }
    }
}

/// Server configuration validation error.
#[derive(Debug, Error)]
pub enum ServerConfigError {
    #[error("non-loopback binding requires authentication")]
    AuthenticationRequired,
    #[error("invalid server bind address `{value}`: {source}")]
    InvalidBind {
        value: String,
        source: std::net::AddrParseError,
    },
    #[error("FIXER_SERVER_BIND is not valid Unicode: {0}")]
    InvalidBindEnvironment(env::VarError),
    #[error("FIXER_SERVER_DATABASE is not valid Unicode: {0}")]
    InvalidDatabaseEnvironment(env::VarError),
    #[error("SQLite database path must not be empty")]
    EmptyDatabasePath,
}

/// Production startup failure.
#[derive(Debug, Error)]
pub enum ServeError {
    #[error("failed to open persistent job store: {0}")]
    Store(#[from] store::StoreError),
    #[error("server I/O failed: {0}")]
    Io(#[from] std::io::Error),
}

/// Opens persistent services, binds, and serves the complete application.
pub async fn serve(config: ServerConfig) -> Result<(), ServeError> {
    let store = SqliteJobStore::open(config.database_path()).await?;
    let runtime = JobRuntime::new(store, DEFAULT_EVENT_CAPACITY);
    let listener = tokio::net::TcpListener::bind(config.bind_addr()).await?;
    let workers = runtime.start_local_workers(DEFAULT_WORKER_COUNT);
    let serve_result = axum::serve(listener, job_app(runtime))
        .with_graceful_shutdown(shutdown_signal())
        .await;
    workers.shutdown().await;
    serve_result?;
    Ok(())
}

async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
}
