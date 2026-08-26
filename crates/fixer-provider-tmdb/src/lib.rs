#![forbid(unsafe_code)]
mod config;
mod error;
mod movie;
mod television;

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
                [MediaKind::Movie, MediaKind::Television],
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
    pub async fn search_television(
        &self,
        request: SearchRequest,
        http: &dyn HttpClient,
    ) -> Result<Vec<Candidate>, TmdbError> {
        television::search(&self.config, request, http).await
    }
    pub async fn fetch_television(
        &self,
        request: FetchRequest,
        http: &dyn HttpClient,
    ) -> Result<fixer_core::Series, TmdbError> {
        television::fetch(&self.config, request, http).await
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
            match request.media_kind() {
                MediaKind::Movie => self.search_movie(request, http).await,
                MediaKind::Television => self.search_television(request, http).await,
                media_kind => {
                    return Err(ProviderError::UnsupportedMedia {
                        provider: self.descriptor.id().clone(),
                        media_kind,
                    });
                }
            }
            .map_err(ProviderError::from)
        })
    }
    fn fetch<'a>(
        &'a self,
        request: FetchRequest,
        http: &'a dyn HttpClient,
    ) -> BoxFuture<'a, Result<MetadataDocument, ProviderError>> {
        Box::pin(async move {
            match request.media_kind() {
                MediaKind::Movie => self
                    .fetch_movie(request, http)
                    .await
                    .map(MetadataDocument::Movie),
                MediaKind::Television => self
                    .fetch_television(request, http)
                    .await
                    .map(MetadataDocument::Television),
                media_kind => {
                    return Err(ProviderError::UnsupportedMedia {
                        provider: self.descriptor.id().clone(),
                        media_kind,
                    });
                }
            }
            .map_err(ProviderError::from)
        })
    }
}
