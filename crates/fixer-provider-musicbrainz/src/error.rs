use fixer_core::{HttpError, ProviderError};
use thiserror::Error;

/// Structured MusicBrainz provider failures.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[non_exhaustive]
pub enum MusicBrainzError {
    #[error("MusicBrainz configuration is invalid: {0}")]
    InvalidConfig(String),
    #[error("MusicBrainz item was not found")]
    NotFound,
    #[error("MusicBrainz rate limit was exceeded")]
    RateLimited,
    #[error("MusicBrainz request timed out")]
    Timeout,
    #[error("MusicBrainz request was blocked by offline mode")]
    Offline,
    #[error("MusicBrainz response was malformed: {0}")]
    MalformedResponse(String),
    #[error("MusicBrainz transport failed: {0}")]
    Transport(String),
    #[error("MusicBrainz returned unexpected status {0}")]
    UnexpectedStatus(u16),
    #[error("MusicBrainz data was invalid: {0}")]
    InvalidData(String),
}

impl MusicBrainzError {
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

impl From<MusicBrainzError> for ProviderError {
    fn from(error: MusicBrainzError) -> Self {
        match error {
            MusicBrainzError::NotFound => ProviderError::NotFound,
            MusicBrainzError::MalformedResponse(message) => ProviderError::InvalidResponse(message),
            MusicBrainzError::InvalidConfig(message) | MusicBrainzError::InvalidData(message) => {
                ProviderError::InvalidInput(message)
            }
            other => ProviderError::Transport(other.to_string()),
        }
    }
}
