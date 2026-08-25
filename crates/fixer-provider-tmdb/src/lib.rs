#![forbid(unsafe_code)]
mod config;
mod error;
mod movie;

pub use config::TmdbConfig;
pub use error::TmdbError;

use fixer_core::{
    BoxFuture, Candidate, FetchRequest, HttpClient, MediaKind, MetadataDocument, Provider,
    ProviderDescriptor, ProviderError, ProviderId, SearchRequest,
};

#[derive(Debug, Clone)]
pub struct TmdbProvider {
    descriptor: ProviderDescriptor,
    config: TmdbConfig,
}
impl TmdbProvider {
    pub fn new(config: TmdbConfig) -> Result<Self, TmdbError> {
        Ok(Self {
            descriptor: ProviderDescriptor::new(
                ProviderId::new("tmdb")
                    .map_err(|error| TmdbError::InvalidConfig(error.to_string()))?,
                "The Movie Database",
                [MediaKind::Movie],
            )
            .map_err(|error| TmdbError::InvalidConfig(error.to_string()))?,
            config,
        })
    }
    pub async fn search_movie(
        &self,
        request: SearchRequest,
        http: &dyn HttpClient,
    ) -> Result<Vec<Candidate>, TmdbError> {
        movie::search(&self.config, request, http).await
    }
    pub async fn fetch_movie(
        &self,
        request: FetchRequest,
        http: &dyn HttpClient,
    ) -> Result<fixer_core::Movie, TmdbError> {
        movie::fetch(&self.config, request, http).await
    }
}
impl Provider for TmdbProvider {
    fn descriptor(&self) -> &ProviderDescriptor {
        &self.descriptor
    }
    fn search<'a>(
        &'a self,
        request: SearchRequest,
        http: &'a dyn HttpClient,
    ) -> BoxFuture<'a, Result<Vec<Candidate>, ProviderError>> {
        Box::pin(async move {
            self.search_movie(request, http)
                .await
                .map_err(ProviderError::from)
        })
    }
    fn fetch<'a>(
        &'a self,
        request: FetchRequest,
        http: &'a dyn HttpClient,
    ) -> BoxFuture<'a, Result<MetadataDocument, ProviderError>> {
        Box::pin(async move {
            self.fetch_movie(request, http)
                .await
                .map(MetadataDocument::Movie)
                .map_err(ProviderError::from)
        })
    }
}
