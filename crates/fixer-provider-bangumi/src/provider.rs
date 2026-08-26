use crate::{BangumiConfig, BangumiError, anime};
use fixer_core::{
    BoxFuture, Candidate, FetchRequest, HttpClient, MediaKind, MetadataDocument, Provider,
    ProviderDescriptor, ProviderError, ProviderId, SearchRequest,
};

/// Bangumi v0 anime provider.
#[derive(Debug, Clone)]
pub struct BangumiProvider {
    descriptor: ProviderDescriptor,
    config: BangumiConfig,
}

impl BangumiProvider {
    /// Constructs a provider with validated identity and endpoint configuration.
    pub fn new(config: BangumiConfig) -> Result<Self, BangumiError> {
        Ok(Self {
            descriptor: ProviderDescriptor::new(
                ProviderId::new("bangumi")
                    .map_err(|error| BangumiError::InvalidConfig(error.to_string()))?,
                "Bangumi",
                [MediaKind::Anime],
            )
            .map_err(|error| BangumiError::InvalidConfig(error.to_string()))?,
            config,
        })
    }

    /// Searches Bangumi anime subjects.
    pub async fn search_anime(
        &self,
        request: SearchRequest,
        http: &dyn HttpClient,
    ) -> Result<Vec<Candidate>, BangumiError> {
        anime::search(&self.config, request, http).await
    }

    /// Fetches one Bangumi anime subject and all supported episode classes.
    pub async fn fetch_anime(
        &self,
        request: FetchRequest,
        http: &dyn HttpClient,
    ) -> Result<fixer_core::AnimeSeries, BangumiError> {
        anime::fetch(&self.config, request, http).await
    }
}

impl Provider for BangumiProvider {
    fn descriptor(&self) -> &ProviderDescriptor {
        &self.descriptor
    }

    fn search<'a>(
        &'a self,
        request: SearchRequest,
        http: &'a dyn HttpClient,
    ) -> BoxFuture<'a, Result<Vec<Candidate>, ProviderError>> {
        Box::pin(async move {
            if request.media_kind() != MediaKind::Anime {
                return Err(ProviderError::UnsupportedMedia {
                    provider: self.descriptor.id().clone(),
                    media_kind: request.media_kind(),
                });
            }
            self.search_anime(request, http)
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
            if request.media_kind() != MediaKind::Anime {
                return Err(ProviderError::UnsupportedMedia {
                    provider: self.descriptor.id().clone(),
                    media_kind: request.media_kind(),
                });
            }
            self.fetch_anime(request, http)
                .await
                .map(MetadataDocument::Anime)
                .map_err(ProviderError::from)
        })
    }
}
