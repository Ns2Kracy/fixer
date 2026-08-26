use crate::MusicBrainzError;
use std::{fmt, time::Duration};
use url::Url;

const DEFAULT_BASE_URL: &str = "https://musicbrainz.org/ws/2/";
const DEFAULT_USER_AGENT: &str = concat!(
    "ns2kracy/fixer/",
    env!("CARGO_PKG_VERSION"),
    " (https://github.com/ns2kracy/fixer)"
);

/// MusicBrainz endpoint, identity, and request pacing configuration.
#[derive(Clone)]
pub struct MusicBrainzConfig {
    base_url: Url,
    user_agent: String,
    minimum_request_interval: Duration,
}

impl Default for MusicBrainzConfig {
    fn default() -> Self {
        Self {
            base_url: Url::parse(DEFAULT_BASE_URL).expect("static MusicBrainz URL"),
            user_agent: DEFAULT_USER_AGENT.to_owned(),
            minimum_request_interval: Duration::from_secs(1),
        }
    }
}

impl fmt::Debug for MusicBrainzConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("MusicBrainzConfig")
            .field("base_url", &self.base_url)
            .field("user_agent", &self.user_agent)
            .field("minimum_request_interval", &self.minimum_request_interval)
            .finish()
    }
}

impl MusicBrainzConfig {
    /// Overrides the API base URL, primarily for fixture servers.
    pub fn with_base_url(mut self, base_url: impl AsRef<str>) -> Result<Self, MusicBrainzError> {
        self.base_url = Url::parse(base_url.as_ref())
            .map_err(|error| MusicBrainzError::InvalidConfig(error.to_string()))?;
        if !matches!(self.base_url.scheme(), "http" | "https") {
            return Err(MusicBrainzError::InvalidConfig(
                "MusicBrainz base URL must use HTTP or HTTPS".to_owned(),
            ));
        }
        Ok(self)
    }

    /// Overrides the mandatory application/version/contact User-Agent.
    pub fn with_user_agent(
        mut self,
        user_agent: impl Into<String>,
    ) -> Result<Self, MusicBrainzError> {
        let user_agent = user_agent.into();
        let meaningful = user_agent.contains('/')
            && user_agent.contains('(')
            && user_agent.contains(')')
            && !user_agent.chars().any(char::is_control);
        if !meaningful {
            return Err(MusicBrainzError::InvalidConfig(
                "MusicBrainz User-Agent must identify application/version and contact information"
                    .to_owned(),
            ));
        }
        self.user_agent = user_agent;
        Ok(self)
    }

    /// Overrides the minimum delay between requests from this provider instance.
    pub fn with_minimum_request_interval(
        mut self,
        interval: Duration,
    ) -> Result<Self, MusicBrainzError> {
        if interval.is_zero() {
            return Err(MusicBrainzError::InvalidConfig(
                "MusicBrainz request interval must be positive".to_owned(),
            ));
        }
        self.minimum_request_interval = interval;
        Ok(self)
    }

    /// Returns the API base URL.
    pub const fn base_url(&self) -> &Url {
        &self.base_url
    }

    /// Returns the mandatory identifiable User-Agent.
    pub fn user_agent(&self) -> &str {
        &self.user_agent
    }

    /// Returns the minimum delay between MusicBrainz requests.
    pub const fn minimum_request_interval(&self) -> Duration {
        self.minimum_request_interval
    }
}
