use crate::{AniListConfig, AniListError, graphql};
use fixer_core::{
    AnimeSeries, BoxFuture, Candidate, FetchRequest, HttpClient, MediaKind, MetadataDocument,
    Provider, ProviderDescriptor, ProviderError, ProviderId, SearchRequest,
};

/// Optional AniList anime metadata provider.
#[derive(Debug, Clone)]
pub struct AniListProvider {
    descriptor: ProviderDescriptor,
    config: AniListConfig,
}

impl AniListProvider {
    /// Constructs an anime-only AniList provider.
    pub fn new(config: AniListConfig) -> Result<Self, AniListError> {
        Ok(Self {
            descriptor: ProviderDescriptor::new(
                ProviderId::new("anilist")
                    .map_err(|error| AniListError::InvalidConfig(error.to_string()))?,
                "AniList",
                [MediaKind::Anime],
            )
            .map_err(|error| AniListError::InvalidConfig(error.to_string()))?,
            config,
        })
    }

    /// Searches AniList anime media.
    pub async fn search_anime(
        &self,
        request: SearchRequest,
        http: &dyn HttpClient,
    ) -> Result<Vec<Candidate>, AniListError> {
        graphql::search(&self.config, request, http).await
    }

    /// Fetches one AniList anime media document.
    pub async fn fetch_anime(
        &self,
        request: FetchRequest,
        http: &dyn HttpClient,
    ) -> Result<AnimeSeries, AniListError> {
        graphql::fetch(&self.config, request, http).await
    }
}

impl Provider for AniListProvider {
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
