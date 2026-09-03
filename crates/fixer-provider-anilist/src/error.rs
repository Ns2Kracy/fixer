use fixer_core::{HttpError, ProviderError};
use thiserror::Error;

/// Structured `AniList` provider failures.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[non_exhaustive]
pub enum AniListError {
    #[error("AniList configuration is invalid: {0}")]
    InvalidConfig(String),
    #[error("AniList authentication failed")]
    Unauthorized,
    #[error("AniList item was not found")]
    NotFound,
    #[error("AniList rate limit was exceeded")]
    RateLimited,
    #[error("AniList request timed out")]
    Timeout,
    #[error("AniList request was blocked by offline mode")]
    Offline,
    #[error("AniList GraphQL failed: {0}")]
    GraphQl(String),
    #[error("AniList response was malformed: {0}")]
    MalformedResponse(String),
    #[error("AniList transport failed: {0}")]
    Transport(String),
    #[error("AniList returned unexpected status {0}")]
    UnexpectedStatus(u16),
    #[error("AniList data was invalid: {0}")]
    InvalidData(String),
}

impl AniListError {
    /// Returns a stable machine-readable failure code.
    pub const fn code(&self) -> &'static str {
        match self {
            Self::InvalidConfig(_) => "invalid_config",
            Self::Unauthorized => "unauthorized",
            Self::NotFound => "not_found",
            Self::RateLimited => "rate_limited",
            Self::Timeout => "timeout",
            Self::Offline => "offline",
            Self::GraphQl(_) => "graphql",
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
            HttpError::Status { status: 404 } => Self::NotFound,
            HttpError::Status { status: 429 } => Self::RateLimited,
            HttpError::Status { status } => Self::UnexpectedStatus(status),
            other => Self::Transport(other.to_string()),
        }
    }
}

impl From<AniListError> for ProviderError {
    fn from(error: AniListError) -> Self {
        match error {
            AniListError::NotFound => Self::NotFound,
            AniListError::GraphQl(message) | AniListError::MalformedResponse(message) => {
                Self::InvalidResponse(message)
            }
            AniListError::InvalidConfig(message) | AniListError::InvalidData(message) => {
                Self::InvalidInput(message)
            }
            other => Self::Transport(other.to_string()),
        }
    }
}
