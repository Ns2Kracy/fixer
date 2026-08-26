use crate::{OpenLibraryConfig, OpenLibraryError, book};
use fixer_core::{
    BookWork, BoxFuture, Candidate, FetchRequest, HttpClient, MediaKind, MetadataDocument,
    Provider, ProviderDescriptor, ProviderError, ProviderId, SearchRequest,
};

/// Open Library work and edition metadata provider.
#[derive(Debug, Clone)]
pub struct OpenLibraryProvider {
    descriptor: ProviderDescriptor,
    config: OpenLibraryConfig,
}

impl OpenLibraryProvider {
    /// Constructs a book-only Open Library provider.
    pub fn new(config: OpenLibraryConfig) -> Result<Self, OpenLibraryError> {
        Ok(Self {
            descriptor: ProviderDescriptor::new(
                ProviderId::new("openlibrary")
                    .map_err(|error| OpenLibraryError::InvalidConfig(error.to_string()))?,
                "Open Library",
                [MediaKind::Book],
            )
            .map_err(|error| OpenLibraryError::InvalidConfig(error.to_string()))?,
            config,
        })
    }

    /// Searches Open Library works while retaining edition fetch identities.
    pub async fn search_book(
        &self,
        request: SearchRequest,
        http: &dyn HttpClient,
    ) -> Result<Vec<Candidate>, OpenLibraryError> {
        book::search(&self.config, request, http).await
    }

    /// Fetches one edition, its work, and referenced authors.
    pub async fn fetch_book(
        &self,
        request: FetchRequest,
        http: &dyn HttpClient,
    ) -> Result<BookWork, OpenLibraryError> {
        book::fetch(&self.config, request, http).await
    }
}

impl Provider for OpenLibraryProvider {
    fn descriptor(&self) -> &ProviderDescriptor {
        &self.descriptor
    }

    fn search<'a>(
        &'a self,
        request: SearchRequest,
        http: &'a dyn HttpClient,
    ) -> BoxFuture<'a, Result<Vec<Candidate>, ProviderError>> {
        Box::pin(async move {
            if request.media_kind() != MediaKind::Book {
                return Err(ProviderError::UnsupportedMedia {
                    provider: self.descriptor.id().clone(),
                    media_kind: request.media_kind(),
                });
            }
            self.search_book(request, http)
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
            if request.media_kind() != MediaKind::Book {
                return Err(ProviderError::UnsupportedMedia {
                    provider: self.descriptor.id().clone(),
                    media_kind: request.media_kind(),
                });
            }
            self.fetch_book(request, http)
                .await
                .map(MetadataDocument::Book)
                .map_err(ProviderError::from)
        })
    }
}
