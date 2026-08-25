use crate::TmdbError;
use std::{env, fmt};
use url::Url;

#[derive(Clone)]
struct SecretToken(String);
impl fmt::Debug for SecretToken {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("[REDACTED]")
    }
}

#[derive(Clone)]
pub struct TmdbConfig {
    token: SecretToken,
    base_url: Url,
    image_base_url: Url,
}
impl fmt::Debug for TmdbConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TmdbConfig")
            .field("token", &self.token)
            .field("base_url", &self.base_url)
            .field("image_base_url", &self.image_base_url)
            .finish()
    }
}
impl TmdbConfig {
    pub fn new(token: impl Into<String>) -> Result<Self, TmdbError> {
        let token = token.into();
        if token.trim().is_empty() || token.chars().any(char::is_control) {
            return Err(TmdbError::MissingToken);
        }
        Ok(Self {
            token: SecretToken(token),
            base_url: Url::parse("https://api.themoviedb.org").expect("static TMDB URL"),
            image_base_url: Url::parse("https://image.tmdb.org/t/p/original/")
                .expect("static TMDB image URL"),
        })
    }
    pub fn from_env() -> Result<Self, TmdbError> {
        env::var("TMDB_API_TOKEN")
            .map_err(|_| TmdbError::MissingToken)
            .and_then(Self::new)
    }
    pub fn with_base_url(mut self, base_url: impl AsRef<str>) -> Result<Self, TmdbError> {
        self.base_url = Url::parse(base_url.as_ref())
            .map_err(|error| TmdbError::InvalidConfig(error.to_string()))?;
        Ok(self)
    }
    pub fn with_image_base_url(mut self, base_url: impl AsRef<str>) -> Result<Self, TmdbError> {
        self.image_base_url = Url::parse(base_url.as_ref())
            .map_err(|error| TmdbError::InvalidConfig(error.to_string()))?;
        Ok(self)
    }
    pub(crate) fn token(&self) -> &str {
        &self.token.0
    }
    pub(crate) fn endpoint(&self, path: &str) -> Result<Url, TmdbError> {
        self.base_url
            .join(path)
            .map_err(|error| TmdbError::InvalidConfig(error.to_string()))
    }
    pub(crate) fn image_url(&self, path: &str) -> Result<String, TmdbError> {
        self.image_base_url
            .join(path.trim_start_matches('/'))
            .map(|url| url.to_string())
            .map_err(|error| TmdbError::InvalidData(error.to_string()))
    }
}
