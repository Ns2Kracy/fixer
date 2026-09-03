use fixer_core::{
    BookEdition, BookWork, BoxFuture, Candidate, ExternalId, FetchRequest, HttpClient, Isbn10,
    Isbn13, LocalizedValue, MediaKind, MetadataDocument, OutputOperation, Provider,
    ProviderDescriptor, ProviderError, ProviderId, ReleaseId, SearchRequest, WorkId,
};
use fixer_provider_local::LocalProvider;
use fixer_sdk::{Fixer, FixtureDocument, FixtureProvider};
use fixer_writer_local::BookWriter;

fn book(work_id: &str, title: &str, isbn_10: &str, isbn_13: &str, publisher: &str) -> BookWork {
    let mut titles = LocalizedValue::new();
    titles.insert("und", title.to_owned()).unwrap();
    BookWork::new(
        WorkId::new(work_id).unwrap(),
        titles,
        Vec::new(),
        vec![
            BookEdition::new(
                ReleaseId::new(format!("{work_id}-edition")).unwrap(),
                Isbn10::new(isbn_10).unwrap(),
                Isbn13::new(isbn_13).unwrap(),
                publisher,
                Vec::new(),
            )
            .unwrap(),
        ],
    )
}

fn fixture(provider: &str, work: BookWork) -> FixtureProvider {
    let isbn = work.editions[0].isbn_13.as_str().to_owned();
    FixtureProvider::new(
        ProviderId::new(provider).unwrap(),
        [FixtureDocument::new(
            ExternalId::new("isbn", isbn).unwrap(),
            MetadataDocument::Book(work),
        )],
    )
    .unwrap()
}

struct FailingBookProvider {
    descriptor: ProviderDescriptor,
}

impl FailingBookProvider {
    fn new(id: &str, network: bool) -> Self {
        Self {
            descriptor: ProviderDescriptor::new(
                ProviderId::new(id).unwrap(),
                "Failing book provider",
                [MediaKind::Book],
            )
            .unwrap()
            .with_network_requirement(network),
        }
    }
}

impl Provider for FailingBookProvider {
    fn descriptor(&self) -> &ProviderDescriptor {
        &self.descriptor
    }

    fn search<'a>(
        &'a self,
        _: SearchRequest,
        _: &'a dyn HttpClient,
    ) -> BoxFuture<'a, Result<Vec<Candidate>, ProviderError>> {
        assert!(
            !self.descriptor.requires_network(),
            "offline book flow invoked a network provider"
        );
        Box::pin(async {
            Err(ProviderError::InvalidResponse(
                "fixture search failure".to_owned(),
            ))
        })
    }

    fn fetch<'a>(
        &'a self,
        _: FetchRequest,
        _: &'a dyn HttpClient,
    ) -> BoxFuture<'a, Result<MetadataDocument, ProviderError>> {
        panic!("failing provider must not be selected for fetch")
    }
}

#[tokio::test]
async fn exact_isbn_outranks_title_and_selected_edition_plans_output() {
    let exact_title = fixture(
        "title-provider",
        book(
            "title-work",
            "The Left Hand of Darkness",
            "0061054887",
            "9780061054884",
            "Title Press",
        ),
    );
    let exact_isbn = fixture(
        "isbn-provider",
        book(
            "isbn-work",
            "A Different Display Title",
            "0441478123",
            "9780441478125",
            "Ace Books",
        ),
    );
    let fixer = Fixer::builder()
        .provider(exact_title)
        .provider(exact_isbn)
        .build()
        .unwrap();
    let isbn = Isbn13::new("9780441478125").unwrap();

    let search = fixer
        .book("The Left Hand of Darkness")
        .isbn(isbn.clone())
        .search()
        .await
        .unwrap();
    let Candidate::Book(first) = &search.candidates()[0] else {
        panic!("expected book candidate");
    };
    assert_eq!(first.external_id.value, isbn.as_str());
    assert_eq!(first.title, "A Different Display Title");

    let resolved = search.select(0).unwrap().fetch_selected().await.unwrap();
    assert_eq!(resolved.value.editions[0].publisher, "Ace Books");
    assert_eq!(
        resolved.provenance.sources_for("book.editions")[0]
            .provider
            .as_str(),
        "isbn-provider"
    );
    let plan = BookWriter::for_isbn(isbn)
        .plan_resolved(&resolved, "library/The Left Hand of Darkness")
        .unwrap();
    assert!(plan.operations().iter().any(|operation| matches!(
        operation,
        OutputOperation::WriteBytes { target, .. } if target == std::path::Path::new("book.opf")
    )));
}

#[tokio::test]
async fn successful_book_result_preserves_partial_provider_failure_warning() {
    let fixer = Fixer::builder()
        .provider(FailingBookProvider::new("failing", false))
        .provider(fixture(
            "working",
            book(
                "work",
                "The Left Hand of Darkness",
                "0441478123",
                "9780441478125",
                "Ace Books",
            ),
        ))
        .build()
        .unwrap();

    let resolved = fixer
        .book("The Left Hand of Darkness")
        .resolve()
        .await
        .unwrap();

    assert!(resolved.warnings.iter().any(|warning| {
        warning.code == "provider_search_failed"
            && warning.message.contains("fixture search failure")
    }));
}

#[tokio::test]
async fn offline_book_flow_resolves_local_work_and_skips_network_provider() {
    let local = LocalProvider::from_book_documents([book(
        "local-work",
        "The Left Hand of Darkness",
        "0441478123",
        "9780441478125",
        "Ace Books",
    )])
    .unwrap();
    let fixer = Fixer::builder()
        .provider(local)
        .provider(FailingBookProvider::new("network", true))
        .offline()
        .build()
        .unwrap();

    let resolved = fixer
        .book("The Left Hand of Darkness")
        .isbn(Isbn13::new("9780441478125").unwrap())
        .resolve()
        .await
        .unwrap();

    assert_eq!(resolved.value.id.as_str(), "local-work");
    assert!(
        resolved
            .warnings
            .iter()
            .any(|warning| warning.code == "offline_provider_skipped")
    );
}
