use crate::AniListError;
use std::fmt;
use url::Url;

const DEFAULT_ENDPOINT: &str = "https://graphql.anilist.co";

/// `AniList` GraphQL endpoint and optional bearer-token configuration.
#[derive(Clone)]
pub struct AniListConfig {
    endpoint: Url,
    access_token: Option<String>,
}

impl Default for AniListConfig {
    fn default() -> Self {
        Self {
            endpoint: Url::parse(DEFAULT_ENDPOINT).expect("static AniList endpoint"),
            access_token: None,
        }
    }
}

impl fmt::Debug for AniListConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AniListConfig")
            .field("endpoint", &self.endpoint)
            .field(
                "access_token",
                &self.access_token.as_ref().map(|_| "[REDACTED]"),
            )
            .finish()
    }
}

impl AniListConfig {
    /// Overrides the GraphQL endpoint, primarily for fixture servers.
    pub fn with_endpoint(mut self, endpoint: impl AsRef<str>) -> Result<Self, AniListError> {
        self.endpoint = Url::parse(endpoint.as_ref())
            .map_err(|error| AniListError::InvalidConfig(error.to_string()))?;
        if !matches!(self.endpoint.scheme(), "http" | "https") {
            return Err(AniListError::InvalidConfig(
                "AniList endpoint must use HTTP or HTTPS".to_owned(),
            ));
        }
        Ok(self)
    }

    /// Sets the optional OAuth bearer token used for authenticated requests.
    pub fn with_access_token(
        mut self,
        access_token: impl Into<String>,
    ) -> Result<Self, AniListError> {
        let access_token = access_token.into();
        if access_token.trim().is_empty() || access_token.chars().any(char::is_control) {
            return Err(AniListError::InvalidConfig(
                "AniList access token must be non-empty and contain no control characters"
                    .to_owned(),
            ));
        }
        self.access_token = Some(access_token);
        Ok(self)
    }

    /// Returns the configured GraphQL endpoint.
    pub const fn endpoint(&self) -> &Url {
        &self.endpoint
    }

    /// Returns the optional bearer token.
    pub fn access_token(&self) -> Option<&str> {
        self.access_token.as_deref()
    }
}
