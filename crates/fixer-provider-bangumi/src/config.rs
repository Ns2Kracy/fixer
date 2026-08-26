use crate::BangumiError;
use std::fmt;
use url::Url;

const DEFAULT_BASE_URL: &str = "https://api.bgm.tv";
const DEFAULT_USER_AGENT: &str = concat!(
    "ns2kracy/fixer/",
    env!("CARGO_PKG_VERSION"),
    " (https://github.com/ns2kracy/fixer)"
);

/// Bangumi endpoint and client identity configuration.
#[derive(Clone)]
pub struct BangumiConfig {
    base_url: Url,
    user_agent: String,
}

impl Default for BangumiConfig {
    fn default() -> Self {
        Self {
            base_url: Url::parse(DEFAULT_BASE_URL).expect("static Bangumi URL"),
            user_agent: DEFAULT_USER_AGENT.to_owned(),
        }
    }
}

impl fmt::Debug for BangumiConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("BangumiConfig")
            .field("base_url", &self.base_url)
            .field("user_agent", &self.user_agent)
            .finish()
    }
}

impl BangumiConfig {
    /// Constructs the documented production endpoint and identifiable User-Agent.
    pub fn new() -> Self {
        Self::default()
    }

    /// Overrides the API endpoint, primarily for fixtures and compatible deployments.
    pub fn with_base_url(mut self, base_url: impl AsRef<str>) -> Result<Self, BangumiError> {
        self.base_url = Url::parse(base_url.as_ref())
            .map_err(|error| BangumiError::InvalidConfig(error.to_string()))?;
        if !matches!(self.base_url.scheme(), "http" | "https") {
            return Err(BangumiError::InvalidConfig(
                "Bangumi base URL must use HTTP or HTTPS".to_owned(),
            ));
        }
        Ok(self)
    }

    /// Overrides the required identifiable non-browser User-Agent.
    pub fn with_user_agent(mut self, user_agent: impl Into<String>) -> Result<Self, BangumiError> {
        let user_agent = user_agent.into();
        if user_agent.trim().is_empty() || user_agent.chars().any(char::is_control) {
            return Err(BangumiError::InvalidConfig(
                "Bangumi User-Agent must be non-empty and contain no control characters".to_owned(),
            ));
        }
        self.user_agent = user_agent;
        Ok(self)
    }

    /// Returns the configured API base URL.
    pub const fn base_url(&self) -> &Url {
        &self.base_url
    }

    /// Returns the identifiable User-Agent sent with API requests.
    pub fn user_agent(&self) -> &str {
        &self.user_agent
    }
}
