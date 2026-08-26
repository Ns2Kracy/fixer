#![forbid(unsafe_code)]

pub mod api;
mod app;
pub mod jobs;
pub mod store;

use std::{env, net::SocketAddr};

pub use app::app;
use thiserror::Error;

const DEFAULT_BIND_ADDR: SocketAddr =
    SocketAddr::new(std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST), 3000);

/// Validated network configuration for the HTTP service.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ServerConfig {
    bind_addr: SocketAddr,
}

impl ServerConfig {
    /// Validates an explicit bind address.
    ///
    /// Authentication is not implemented yet, so this server version accepts
    /// only loopback listeners.
    pub fn new(bind_addr: SocketAddr) -> Result<Self, ServerConfigError> {
        if !bind_addr.ip().is_loopback() {
            return Err(ServerConfigError::AuthenticationRequired);
        }
        Ok(Self { bind_addr })
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

    /// Loads `FIXER_SERVER_BIND`, falling back to `127.0.0.1:3000`.
    pub fn from_env() -> Result<Self, ServerConfigError> {
        match env::var("FIXER_SERVER_BIND") {
            Ok(value) => Self::parse(&value),
            Err(env::VarError::NotPresent) => Ok(Self::default()),
            Err(source) => Err(ServerConfigError::InvalidEnvironment(source)),
        }
    }

    /// Returns the validated listener address.
    pub const fn bind_addr(&self) -> SocketAddr {
        self.bind_addr
    }
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            bind_addr: DEFAULT_BIND_ADDR,
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
    InvalidEnvironment(env::VarError),
}

/// Binds and serves the application using validated configuration.
pub async fn serve(config: ServerConfig) -> std::io::Result<()> {
    let listener = tokio::net::TcpListener::bind(config.bind_addr()).await?;
    axum::serve(listener, app()).await
}
