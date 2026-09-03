use std::net::{IpAddr, SocketAddr};

use axum::http::{HeaderMap, HeaderName, header};
use ipnet::IpNet;
use thiserror::Error;

/// Explicit policy for accepting one client-IP header from exact proxy ranges.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TrustedProxyPolicy {
    trusted_ranges: Vec<IpNet>,
    client_ip_header: Option<HeaderName>,
}

impl TrustedProxyPolicy {
    /// Ignores all proxy identity headers.
    pub const fn disabled() -> Self {
        Self {
            trusted_ranges: Vec::new(),
            client_ip_header: None,
        }
    }

    pub fn new<I, S>(ranges: I, client_ip_header: &str) -> Result<Self, TrustedProxyError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let header = client_ip_header
            .parse::<HeaderName>()
            .map_err(|_| TrustedProxyError::InvalidHeader(client_ip_header.to_owned()))?;
        if matches!(
            header,
            header::AUTHORIZATION | header::COOKIE | header::PROXY_AUTHORIZATION
        ) {
            return Err(TrustedProxyError::CredentialHeader(header.to_string()));
        }
        let mut trusted_ranges = ranges
            .into_iter()
            .map(|range| {
                let range = range.as_ref();
                range
                    .parse::<IpNet>()
                    .map_err(|_| TrustedProxyError::InvalidRange(range.to_owned()))
            })
            .collect::<Result<Vec<_>, _>>()?;
        trusted_ranges.sort_by_key(ToString::to_string);
        trusted_ranges.dedup();
        if trusted_ranges.is_empty() {
            return Err(TrustedProxyError::NoRanges);
        }
        Ok(Self {
            trusted_ranges,
            client_ip_header: Some(header),
        })
    }

    pub const fn is_enabled(&self) -> bool {
        self.client_ip_header.is_some()
    }

    /// Returns a configured forwarded identity only for a trusted socket peer.
    /// Invalid or multi-hop values fail closed to the socket peer identity.
    pub fn client_ip(&self, peer: SocketAddr, headers: &HeaderMap) -> IpAddr {
        let Some(header) = &self.client_ip_header else {
            return peer.ip();
        };
        if !self
            .trusted_ranges
            .iter()
            .any(|network| network.contains(&peer.ip()))
        {
            return peer.ip();
        }
        headers
            .get(header)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse::<IpAddr>().ok())
            .unwrap_or_else(|| peer.ip())
    }
}

#[derive(Debug, Error)]
pub enum TrustedProxyError {
    #[error("trusted proxy configuration requires at least one CIDR range")]
    NoRanges,
    #[error("trusted proxy CIDR `{0}` is invalid")]
    InvalidRange(String),
    #[error("trusted proxy identity header `{0}` is invalid")]
    InvalidHeader(String),
    #[error("trusted proxy identity must not use credential header `{0}`")]
    CredentialHeader(String),
}
