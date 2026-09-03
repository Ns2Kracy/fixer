//! Default HTTP transport configuration.

use std::{fmt, time::Duration};
use thiserror::Error;

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);
const DEFAULT_USER_AGENT: &str = concat!("fixer/", env!("CARGO_PKG_VERSION"));

/// Errors produced while validating HTTP configuration.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum HttpConfigError {
    #[error("unsupported or invalid proxy URL")]
    InvalidProxy,
    #[error("HTTP timeout must be greater than zero")]
    InvalidTimeout,
    #[error("HTTP user agent must not be empty or contain control characters")]
    InvalidUserAgent,
    #[error("failed to construct HTTP client: {0}")]
    Client(String),
}

#[derive(Clone, PartialEq, Eq)]
pub struct ProxyAddress(String);
impl ProxyAddress {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl fmt::Debug for ProxyAddress {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("[REDACTED]")
    }
}

/// Configuration for the rustls-backed default HTTP client.
#[derive(Clone, PartialEq, Eq)]
pub struct HttpConfig {
    pub(crate) timeout: Duration,
    pub(crate) user_agent: String,
    pub(crate) proxy: Option<ProxyAddress>,
}

impl Default for HttpConfig {
    fn default() -> Self {
        Self {
            timeout: DEFAULT_TIMEOUT,
            user_agent: DEFAULT_USER_AGENT.to_owned(),
            proxy: None,
        }
    }
}

impl fmt::Debug for HttpConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("HttpConfig")
            .field("timeout", &self.timeout)
            .field("user_agent", &self.user_agent)
            .field("proxy", &self.proxy)
            .finish()
    }
}

impl HttpConfig {
    /// Sets a positive request timeout.
    pub const fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Sets one global HTTP, HTTPS, or SOCKS proxy.
    pub fn with_proxy(mut self, proxy: impl Into<String>) -> Result<Self, HttpConfigError> {
        let proxy = proxy.into();
        let scheme = proxy
            .split_once("://")
            .map(|(scheme, _)| scheme.to_ascii_lowercase())
            .ok_or(HttpConfigError::InvalidProxy)?;
        if !matches!(
            scheme.as_str(),
            "http" | "https" | "socks4" | "socks4a" | "socks5" | "socks5h"
        ) || reqwest::Proxy::all(&proxy).is_err()
        {
            return Err(HttpConfigError::InvalidProxy);
        }
        self.proxy = Some(ProxyAddress(proxy));
        Ok(self)
    }

    /// Sets a non-empty HTTP user agent.
    pub fn with_user_agent(
        mut self,
        user_agent: impl Into<String>,
    ) -> Result<Self, HttpConfigError> {
        let user_agent = user_agent.into();
        if user_agent.trim().is_empty() || user_agent.chars().any(char::is_control) {
            return Err(HttpConfigError::InvalidUserAgent);
        }
        self.user_agent = user_agent;
        Ok(self)
    }

    pub(crate) const fn validate(&self) -> Result<(), HttpConfigError> {
        if self.timeout.is_zero() {
            Err(HttpConfigError::InvalidTimeout)
        } else {
            Ok(())
        }
    }
}
