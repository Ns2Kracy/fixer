use fixer_core::{HttpError, ProviderError};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum TmdbError {
    #[error("TMDB API token is missing")]
    MissingToken,
    #[error("TMDB configuration is invalid: {0}")]
    InvalidConfig(String),
    #[error("TMDB authentication failed")]
    Unauthorized,
    #[error("TMDB item was not found")]
    NotFound,
    #[error("TMDB rate limit was exceeded")]
    RateLimited,
    #[error("TMDB request timed out")]
    Timeout,
    #[error("TMDB search returned no results")]
    EmptyResults,
    #[error("TMDB response was malformed: {0}")]
    MalformedResponse(String),
    #[error("TMDB transport failed: {0}")]
    Transport(String),
    #[error("TMDB returned unexpected status {0}")]
    UnexpectedStatus(u16),
    #[error("TMDB data was invalid: {0}")]
    InvalidData(String),
}
impl TmdbError {
    pub const fn code(&self) -> &'static str {
        match self {
            Self::MissingToken => "missing_token",
            Self::InvalidConfig(_) => "invalid_config",
            Self::Unauthorized => "unauthorized",
            Self::NotFound => "not_found",
            Self::RateLimited => "rate_limited",
            Self::Timeout => "timeout",
            Self::EmptyResults => "empty_results",
            Self::MalformedResponse(_) => "malformed_response",
            Self::Transport(_) => "transport",
            Self::UnexpectedStatus(_) => "unexpected_status",
            Self::InvalidData(_) => "invalid_data",
        }
    }
    pub(crate) fn from_http(error: HttpError) -> Self {
        match error {
            HttpError::Timeout => Self::Timeout,
            HttpError::Status { status: 401 } => Self::Unauthorized,
            HttpError::Status { status: 404 } => Self::NotFound,
            HttpError::Status { status: 429 } => Self::RateLimited,
            HttpError::Status { status } => Self::UnexpectedStatus(status),
            other => Self::Transport(other.to_string()),
        }
    }
}
impl From<TmdbError> for ProviderError {
    fn from(error: TmdbError) -> Self {
        match error {
            TmdbError::NotFound => ProviderError::NotFound,
            TmdbError::MalformedResponse(message) => ProviderError::InvalidResponse(message),
            TmdbError::InvalidData(message) | TmdbError::InvalidConfig(message) => {
                ProviderError::InvalidInput(message)
            }
            other => ProviderError::Transport(other.to_string()),
        }
    }
}
