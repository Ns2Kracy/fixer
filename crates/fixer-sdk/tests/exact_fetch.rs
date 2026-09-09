use fixer_core::{
    BoxFuture, Candidate, ExternalId, FetchRequest, HttpClient, MediaKind, MetadataDocument, Movie,
    Provider, ProviderDescriptor, ProviderError, ProviderId, ProviderTarget, SearchRequest, WorkId,
};
use fixer_sdk::{Fixer, SdkError};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

struct TrackingProvider {
    descriptor: ProviderDescriptor,
    search_calls: Arc<AtomicUsize>,
    fetch_calls: Arc<AtomicUsize>,
}

impl Provider for TrackingProvider {
    fn descriptor(&self) -> &ProviderDescriptor {
        &self.descriptor
    }

    fn search<'a>(
        &'a self,
        _request: SearchRequest,
        _http: &'a dyn HttpClient,
    ) -> BoxFuture<'a, Result<Vec<Candidate>, ProviderError>> {
        Box::pin(async move {
            self.search_calls.fetch_add(1, Ordering::SeqCst);
            Ok(Vec::new())
        })
    }

    fn fetch<'a>(
        &'a self,
        request: FetchRequest,
        _http: &'a dyn HttpClient,
    ) -> BoxFuture<'a, Result<MetadataDocument, ProviderError>> {
        Box::pin(async move {
            self.fetch_calls.fetch_add(1, Ordering::SeqCst);
            assert_eq!(request.external_id.value, "329865");
            let mut titles = fixer_core::LocalizedValue::new();
            titles.insert("en", "Arrival".to_owned())?;
            Ok(MetadataDocument::Movie(Movie::new(
                WorkId::new("tmdb:329865")?,
                titles,
            )))
        })
    }
}

#[tokio::test]
async fn exact_fetch_calls_only_the_selected_provider() {
    let search_calls = Arc::new(AtomicUsize::new(0));
    let fetch_calls = Arc::new(AtomicUsize::new(0));
    let provider = TrackingProvider {
        descriptor: ProviderDescriptor::new(
            ProviderId::new("tmdb").unwrap(),
            "TMDB",
            [MediaKind::Movie],
        )
        .unwrap(),
        search_calls: Arc::clone(&search_calls),
        fetch_calls: Arc::clone(&fetch_calls),
    };
    let fixer = Fixer::builder()
        .provider(provider)
        .offline()
        .build()
        .unwrap();
    let target = ProviderTarget::tmdb(MediaKind::Movie, "329865").unwrap();

    let document = fixer.fetch_exact(&target).await.unwrap();

    assert_eq!(document.media_kind(), MediaKind::Movie);
    assert_eq!(search_calls.load(Ordering::SeqCst), 0);
    assert_eq!(fetch_calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn exact_fetch_rejects_an_unregistered_provider() {
    let provider = fixer_sdk::FixtureProvider::new(
        ProviderId::new("local").unwrap(),
        Vec::<fixer_sdk::FixtureDocument>::new(),
    )
    .unwrap();
    let fixer = Fixer::builder()
        .provider(provider)
        .offline()
        .build()
        .unwrap();
    let target = ProviderTarget::new(
        MediaKind::Movie,
        ProviderId::new("tmdb").unwrap(),
        ExternalId::new("tmdb", "329865").unwrap(),
    )
    .unwrap();

    let error = fixer.fetch_exact(&target).await.unwrap_err();

    assert!(matches!(error, SdkError::ProviderNotFound(id) if id.as_str() == "tmdb"));
}
