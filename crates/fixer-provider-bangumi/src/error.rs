use fixer_core::{HttpError, ProviderError};
use thiserror::Error;

/// Structured Bangumi provider failures.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[non_exhaustive]
pub enum BangumiError {
    #[error("Bangumi configuration is invalid: {0}")]
    InvalidConfig(String),
    #[error("Bangumi authentication failed")]
    Unauthorized,
    #[error("Bangumi request was forbidden")]
    Forbidden,
    #[error("Bangumi item was not found")]
    NotFound,
    #[error("Bangumi rate limit was exceeded")]
    RateLimited,
    #[error("Bangumi request timed out")]
    Timeout,
    #[error("Bangumi request was blocked by offline mode")]
    Offline,
    #[error("Bangumi response was malformed: {0}")]
    MalformedResponse(String),
    #[error("Bangumi transport failed: {0}")]
    Transport(String),
    #[error("Bangumi returned unexpected status {0}")]
    UnexpectedStatus(u16),
    #[error("Bangumi data was invalid: {0}")]
    InvalidData(String),
}

impl BangumiError {
    /// Returns a stable machine-readable failure code.
    pub const fn code(&self) -> &'static str {
        match self {
            Self::InvalidConfig(_) => "invalid_config",
            Self::Unauthorized => "unauthorized",
            Self::Forbidden => "forbidden",
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
            HttpError::Status { status: 401 } => Self::Unauthorized,
            HttpError::Status { status: 403 } => Self::Forbidden,
            HttpError::Status { status: 404 } => Self::NotFound,
            HttpError::Status { status: 429 } => Self::RateLimited,
            HttpError::Status { status } => Self::UnexpectedStatus(status),
            other => Self::Transport(other.to_string()),
        }
    }
}

impl From<BangumiError> for ProviderError {
    fn from(error: BangumiError) -> Self {
        match error {
            BangumiError::NotFound => Self::NotFound,
            BangumiError::MalformedResponse(message) => Self::InvalidResponse(message),
            BangumiError::InvalidConfig(message) | BangumiError::InvalidData(message) => {
                Self::InvalidInput(message)
            }
            other => Self::Transport(other.to_string()),
        }
    }
}
