use crate::OpenLibraryError;
use std::fmt;
use url::Url;

const DEFAULT_API_BASE_URL: &str = "https://openlibrary.org/";
const DEFAULT_COVER_BASE_URL: &str = "https://covers.openlibrary.org/b/";
const DEFAULT_USER_AGENT: &str = concat!(
    "ns2kracy/fixer/",
    env!("CARGO_PKG_VERSION"),
    " (https://github.com/ns2kracy/fixer)"
);

/// Open Library API, cover service, and client identity configuration.
#[derive(Clone)]
pub struct OpenLibraryConfig {
    api_base_url: Url,
    cover_base_url: Url,
    user_agent: String,
}

impl Default for OpenLibraryConfig {
    fn default() -> Self {
        Self {
            api_base_url: Url::parse(DEFAULT_API_BASE_URL).expect("static Open Library API URL"),
            cover_base_url: Url::parse(DEFAULT_COVER_BASE_URL)
                .expect("static Open Library cover URL"),
            user_agent: DEFAULT_USER_AGENT.to_owned(),
        }
    }
}

impl fmt::Debug for OpenLibraryConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OpenLibraryConfig")
            .field("api_base_url", &self.api_base_url)
            .field("cover_base_url", &self.cover_base_url)
            .field("user_agent", &self.user_agent)
            .finish()
    }
}

impl OpenLibraryConfig {
    /// Overrides the API base URL, primarily for fixture servers.
    pub fn with_api_base_url(
        mut self,
        base_url: impl AsRef<str>,
    ) -> Result<Self, OpenLibraryError> {
        self.api_base_url = validated_http_url(base_url.as_ref(), "Open Library API")?;
        Ok(self)
    }

    /// Overrides the cover service base URL, primarily for fixture servers.
    pub fn with_cover_base_url(
        mut self,
        base_url: impl AsRef<str>,
    ) -> Result<Self, OpenLibraryError> {
        self.cover_base_url = validated_http_url(base_url.as_ref(), "Open Library cover")?;
        Ok(self)
    }

    /// Overrides the application/version/contact User-Agent.
    pub fn with_user_agent(
        mut self,
        user_agent: impl Into<String>,
    ) -> Result<Self, OpenLibraryError> {
        let user_agent = user_agent.into();
        let identifiable = user_agent.contains('/')
            && user_agent.contains('(')
            && user_agent.contains(')')
            && !user_agent.chars().any(char::is_control);
        if !identifiable {
            return Err(OpenLibraryError::InvalidConfig(
                "Open Library User-Agent must identify application/version and contact information"
                    .to_owned(),
            ));
        }
        self.user_agent = user_agent;
        Ok(self)
    }

    /// Returns the API base URL.
    pub const fn api_base_url(&self) -> &Url {
        &self.api_base_url
    }

    /// Returns the cover service base URL.
    pub const fn cover_base_url(&self) -> &Url {
        &self.cover_base_url
    }

    /// Returns the identifiable User-Agent.
    pub fn user_agent(&self) -> &str {
        &self.user_agent
    }
}

fn validated_http_url(value: &str, service: &str) -> Result<Url, OpenLibraryError> {
    let url =
        Url::parse(value).map_err(|error| OpenLibraryError::InvalidConfig(error.to_string()))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(OpenLibraryError::InvalidConfig(format!(
            "{service} base URL must use HTTP or HTTPS"
        )));
    }
    Ok(url)
}
