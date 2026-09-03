use fixer_core::{HttpError, ProviderError};
use thiserror::Error;

/// Structured Open Library provider failures.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[non_exhaustive]
pub enum OpenLibraryError {
    #[error("Open Library configuration is invalid: {0}")]
    InvalidConfig(String),
    #[error("Open Library item was not found")]
    NotFound,
    #[error("Open Library rate limit was exceeded")]
    RateLimited,
    #[error("Open Library request timed out")]
    Timeout,
    #[error("Open Library request was blocked by offline mode")]
    Offline,
    #[error("Open Library response was malformed: {0}")]
    MalformedResponse(String),
    #[error("Open Library transport failed: {0}")]
    Transport(String),
    #[error("Open Library returned unexpected status {0}")]
    UnexpectedStatus(u16),
    #[error("Open Library data was invalid: {0}")]
    InvalidData(String),
}

impl OpenLibraryError {
    /// Returns a stable machine-readable failure code.
    pub const fn code(&self) -> &'static str {
        match self {
            Self::InvalidConfig(_) => "invalid_config",
            Self::NotFound => "not_found",
            Self::RateLimited => "rate_limited",
            Self::Timeout => "timeout",
            Self::Offline => "offline",
            Self::MalformedResponse(_) => "malformed_response",
            Self::Transport(_) => "transport",
            Self::UnexpectedStatus(_) => "unexpected_status",
            Self::InvalidData(_) => "invalid_data",
        }
    }

    /// Converts runtime-neutral transport failures without losing their category.
    pub fn from_http(error: HttpError) -> Self {
        match error {
            HttpError::Offline => Self::Offline,
            HttpError::Timeout => Self::Timeout,
            HttpError::Status { status: 404 } => Self::NotFound,
            HttpError::Status { status: 429 } => Self::RateLimited,
            HttpError::Status { status } => Self::UnexpectedStatus(status),
            other => Self::Transport(other.to_string()),
        }
    }
}

impl From<OpenLibraryError> for ProviderError {
    fn from(error: OpenLibraryError) -> Self {
        match error {
            OpenLibraryError::NotFound => Self::NotFound,
            OpenLibraryError::MalformedResponse(message) => Self::InvalidResponse(message),
            OpenLibraryError::InvalidConfig(message) | OpenLibraryError::InvalidData(message) => {
                Self::InvalidInput(message)
            }
            other => Self::Transport(other.to_string()),
        }
    }
}
