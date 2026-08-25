//! Runtime-neutral HTTP request and response protocol.

use crate::{BoxFuture, CoreError};
use serde::{Deserialize, Serialize};
use std::fmt;
use thiserror::Error;

/// Supported HTTP methods.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum HttpMethod {
    Get,
    Post,
    Put,
    Patch,
    Delete,
    Head,
}

/// A request or response header with redacted debug output for sensitive names.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Header {
    name: String,
    value: String,
}

impl Header {
    /// Constructs a validated header pair.
    pub fn new(name: impl Into<String>, value: impl Into<String>) -> Result<Self, CoreError> {
        let name = name.into();
        let value = value.into();
        let valid_name = !name.is_empty()
            && name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&byte));
        let valid_value = !value.contains(['\r', '\n', '\0']);
        if !valid_name || !valid_value {
            return Err(CoreError::InvalidDomainValue {
                field: "http.header",
                value: name,
            });
        }
        Ok(Self { name, value })
    }
    /// Returns the header name.
    pub fn name(&self) -> &str {
        &self.name
    }
    /// Returns the value for transport implementations.
    pub fn value(&self) -> &str {
        &self.value
    }
    fn is_sensitive(&self) -> bool {
        let name = self.name.to_ascii_lowercase();
        matches!(
            name.as_str(),
            "authorization"
                | "proxy-authorization"
                | "cookie"
                | "set-cookie"
                | "x-api-key"
                | "api-key"
        ) || name.contains("token")
            || name.contains("secret")
    }
}

impl fmt::Debug for Header {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = if self.is_sensitive() {
            "[REDACTED]"
        } else {
            self.value.as_str()
        };
        formatter
            .debug_struct("Header")
            .field("name", &self.name)
            .field("value", &value)
            .finish()
    }
}

/// A transport-independent HTTP request.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HttpRequest {
    pub method: HttpMethod,
    pub url: String,
    pub headers: Vec<Header>,
    pub body: Vec<u8>,
}

impl HttpRequest {
    /// Constructs an HTTP request.
    pub fn new(method: HttpMethod, url: impl Into<String>) -> Self {
        Self {
            method,
            url: url.into(),
            headers: Vec::new(),
            body: Vec::new(),
        }
    }
    /// Appends one header.
    pub fn with_header(mut self, header: Header) -> Self {
        self.headers.push(header);
        self
    }
    /// Replaces the request body.
    pub fn with_body(mut self, body: impl Into<Vec<u8>>) -> Self {
        self.body = body.into();
        self
    }
}

impl fmt::Debug for HttpRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("HttpRequest")
            .field("method", &self.method)
            .field("url", &redact_url(&self.url))
            .field("headers", &self.headers)
            .field("body_length", &self.body.len())
            .finish()
    }
}

fn redact_url(url: &str) -> String {
    let Some((prefix, query)) = url.split_once('?') else {
        return url.to_owned();
    };
    let redacted = query
        .split('&')
        .map(|pair| {
            let Some((key, value)) = pair.split_once('=') else {
                return pair.to_owned();
            };
            let lowered = key.to_ascii_lowercase();
            if lowered.contains("key") || lowered.contains("token") || lowered.contains("secret") {
                format!("{key}=[REDACTED]")
            } else {
                format!("{key}={value}")
            }
        })
        .collect::<Vec<_>>()
        .join("&");
    format!("{prefix}?{redacted}")
}

/// A transport-independent HTTP response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HttpResponse {
    pub status: u16,
    pub headers: Vec<Header>,
    pub body: Vec<u8>,
}
impl HttpResponse {
    /// Constructs an empty response.
    pub const fn new(status: u16) -> Self {
        Self {
            status,
            headers: Vec::new(),
            body: Vec::new(),
        }
    }
    /// Replaces the response body.
    pub fn with_body(mut self, body: impl Into<Vec<u8>>) -> Self {
        self.body = body.into();
        self
    }
}

/// Structured HTTP transport failures.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[non_exhaustive]
pub enum HttpError {
    #[error("offline mode prevents HTTP requests")]
    Offline,
    #[error("HTTP request timed out")]
    Timeout,
    #[error("HTTP response returned non-success status {status}")]
    Status { status: u16 },
    #[error("HTTP transport failed: {0}")]
    Transport(String),
    #[error("HTTP request or response was invalid: {0}")]
    InvalidMessage(String),
}

/// Runtime-neutral HTTP transport contract.
pub trait HttpClient: Send + Sync {
    fn execute<'a>(
        &'a self,
        request: HttpRequest,
    ) -> BoxFuture<'a, Result<HttpResponse, HttpError>>;
}
