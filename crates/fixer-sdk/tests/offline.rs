use fixer_core::{
    BoxFuture, Candidate, FetchRequest, HttpClient, MediaKind, MetadataDocument, Provider,
    ProviderDescriptor, ProviderError, ProviderId, SearchRequest,
};
use fixer_sdk::{Fixer, FixtureDocument, FixtureProvider};

struct NetworkProvider {
    descriptor: ProviderDescriptor,
}
impl NetworkProvider {
    fn new() -> Self {
        Self {
            descriptor: ProviderDescriptor::new(
                ProviderId::new("network").unwrap(),
                "Network",
                [MediaKind::Movie],
            )
            .unwrap(),
        }
    }
}
impl Provider for NetworkProvider {
    fn descriptor(&self) -> &ProviderDescriptor {
        &self.descriptor
    }
    fn search<'a>(
        &'a self,
        _: SearchRequest,
        _: &'a dyn HttpClient,
    ) -> BoxFuture<'a, Result<Vec<Candidate>, ProviderError>> {
        panic!("offline mode invoked a network provider")
    }
    fn fetch<'a>(
        &'a self,
        _: FetchRequest,
        _: &'a dyn HttpClient,
    ) -> BoxFuture<'a, Result<MetadataDocument, ProviderError>> {
        panic!("offline mode fetched a network provider")
    }
}

#[tokio::test]
async fn offline_skips_network_providers_and_warns() {
    let mut titles = fixer_core::LocalizedValue::new();
    titles.insert("en", "Local Movie".to_owned()).unwrap();
    let document = FixtureDocument::new(
        fixer_core::ExternalId::new("local", "movie-1").unwrap(),
        MetadataDocument::Movie(fixer_core::Movie::new(
            fixer_core::WorkId::new("movie-1").unwrap(),
            titles,
        )),
    );
    let local = FixtureProvider::new(ProviderId::new("local").unwrap(), [document]).unwrap();
    let fixer = Fixer::builder()
        .provider(local)
        .provider(NetworkProvider::new())
        .offline()
        .build()
        .unwrap();
    let search = fixer.movie("Local Movie").search().await.unwrap();
    assert!(
        search
            .warnings()
            .iter()
            .any(|warning| warning.code == "offline_provider_skipped")
    );

    let outcome = search.select(0).unwrap().fetch_selected().await.unwrap();
    assert!(
        outcome
            .warnings
            .iter()
            .any(|warning| warning.code == "offline_provider_skipped")
    );
}
