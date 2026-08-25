//! Tokio-powered orchestration and ergonomic typed flows for Fixer.

#![forbid(unsafe_code)]

mod builder;
pub mod fixture;
mod orchestrator;
pub mod output;
pub mod query;

pub use builder::FixerBuilder;
pub use fixture::{FixtureDocument, FixtureProvider};
pub use query::movie::{MovieQuery, MovieSearch, SelectedMovie};

use fixer_core::{
    CoreError, HttpClient, HttpError, HttpRequest, HttpResponse, LanguageTag, Provider,
    ProviderError, ProviderId,
};
use std::sync::Arc;
use thiserror::Error;

/// SDK orchestration failures.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum SdkError {
    #[error("at least one provider is required")]
    NoProviders,
    #[error("duplicate provider ID `{0}`")]
    DuplicateProvider(ProviderId),
    #[error("no matching candidates were found")]
    NoCandidates,
    #[error("candidate index {index} is out of bounds for {length} candidates")]
    CandidateOutOfBounds { index: usize, length: usize },
    #[error("provider `{0}` is not registered")]
    ProviderNotFound(ProviderId),
    #[error(transparent)]
    Core(#[from] CoreError),
    #[error(transparent)]
    Provider(#[from] ProviderError),
    #[error("all provider operations failed: {0:?}")]
    AllProvidersFailed(Vec<String>),
    #[error("provider returned a non-movie document")]
    UnexpectedDocument,
    #[error("movie merge failed: {0}")]
    Merge(String),
    #[error("invalid HTTP configuration: {0}")]
    HttpConfig(String),
}

struct DisabledHttpClient;
impl HttpClient for DisabledHttpClient {
    fn execute<'a>(
        &'a self,
        _request: HttpRequest,
    ) -> fixer_core::BoxFuture<'a, Result<HttpResponse, HttpError>> {
        Box::pin(async { Err(HttpError::Offline) })
    }
}

/// Configured SDK entry point.
#[derive(Clone)]
pub struct Fixer {
    pub(crate) providers: Arc<[Arc<dyn Provider>]>,
    pub(crate) preferred_languages: Arc<[LanguageTag]>,
    pub(crate) http: Arc<dyn HttpClient>,
    pub(crate) offline: bool,
}

impl Fixer {
    /// Starts validated SDK construction.
    pub fn builder() -> FixerBuilder {
        FixerBuilder::default()
    }

    pub(crate) fn new(
        providers: Vec<Arc<dyn Provider>>,
        preferred_languages: Vec<LanguageTag>,
        http: Option<Arc<dyn HttpClient>>,
        offline: bool,
        proxy: Option<String>,
        timeout: Option<std::time::Duration>,
    ) -> Result<Self, SdkError> {
        let http = match http {
            Some(http) => http,
            None if offline => Arc::new(DisabledHttpClient),
            None => {
                let mut config = fixer_http::HttpConfig::default();
                if let Some(proxy) = proxy {
                    config = config
                        .with_proxy(proxy)
                        .map_err(|error| SdkError::HttpConfig(error.to_string()))?;
                }
                if let Some(timeout) = timeout {
                    config = config.with_timeout(timeout);
                }
                Arc::new(
                    fixer_http::ReqwestHttpClient::new(config)
                        .map_err(|error| SdkError::HttpConfig(error.to_string()))?,
                )
            }
        };
        Ok(Self {
            providers: providers.into(),
            preferred_languages: preferred_languages.into(),
            http,
            offline,
        })
    }

    /// Starts an ergonomic typed movie query.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use fixer_sdk::Fixer;
    /// # async fn example(fixer: Fixer) -> Result<(), fixer_sdk::SdkError> {
    /// let outcome = fixer.movie("花样年华").year(2000).resolve().await?;
    /// assert_eq!(outcome.value().release_year(), Some(2000));
    /// # Ok(())
    /// # }
    /// ```
    pub fn movie(&self, title: impl Into<String>) -> MovieQuery {
        MovieQuery::new(self.clone(), title.into())
    }
}
