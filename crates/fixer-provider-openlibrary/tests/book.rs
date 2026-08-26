use fixer_core::{
    Candidate, CreditRole, ExternalId, FetchRequest, MediaKind, MetadataDocument, Provider,
    ProviderError, SearchRequest,
};
use fixer_http::{HttpConfig, ReqwestHttpClient};
use fixer_provider_openlibrary::{OpenLibraryConfig, OpenLibraryError, OpenLibraryProvider};
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{header, method, path, query_param},
};

fn http() -> ReqwestHttpClient {
    ReqwestHttpClient::new(HttpConfig::default()).unwrap()
}

fn fixture_config(server: &MockServer) -> OpenLibraryConfig {
    OpenLibraryConfig::default()
        .with_api_base_url(format!("{}/", server.uri()))
        .unwrap()
        .with_cover_base_url(format!("{}/covers/", server.uri()))
        .unwrap()
}

async fn mount_fetch_chain(server: &MockServer) {
    Mock::given(method("GET"))
        .and(path("/isbn/9780441478125.json"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_raw(include_str!("fixtures/edition.json"), "application/json"),
        )
        .mount(server)
        .await;
    Mock::given(method("GET"))
        .and(path("/works/OL27448W.json"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_raw(include_str!("fixtures/work.json"), "application/json"),
        )
        .mount(server)
        .await;
    Mock::given(method("GET"))
        .and(path("/authors/OL31353A.json"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_raw(include_str!("fixtures/author.json"), "application/json"),
        )
        .mount(server)
        .await;
}

#[tokio::test]
async fn search_requests_selective_fields_and_emits_isbn_edition_identity() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/search.json"))
        .and(query_param("title", "The Left Hand of Darkness"))
        .and(query_param(
            "fields",
            "key,title,first_publish_year,edition_key,isbn",
        ))
        .and(query_param("limit", "25"))
        .and(header(
            "user-agent",
            "ns2kracy/fixer/0.1.0 (https://github.com/ns2kracy/fixer)",
        ))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_raw(include_str!("fixtures/search.json"), "application/json"),
        )
        .mount(&server)
        .await;
    let provider = OpenLibraryProvider::new(fixture_config(&server)).unwrap();

    let candidates = provider
        .search_book(
            SearchRequest::book("The Left Hand of Darkness", Some(1969)).unwrap(),
            &http(),
        )
        .await
        .unwrap();

    let Candidate::Book(candidate) = &candidates[0] else {
        panic!("expected book candidate");
    };
    assert_eq!(candidate.provider.as_str(), "openlibrary");
    assert_eq!(candidate.external_id.namespace, "isbn");
    assert_eq!(candidate.external_id.value, "9780441478125");
    assert_eq!(candidate.title, "The Left Hand of Darkness");
    assert_eq!(candidate.year, Some(1969));
}

#[tokio::test]
async fn fetch_preserves_work_edition_author_isbns_and_cover_reference() {
    let server = MockServer::start().await;
    mount_fetch_chain(&server).await;
    let provider = OpenLibraryProvider::new(fixture_config(&server)).unwrap();

    let book = provider
        .fetch_book(
            FetchRequest::new(
                MediaKind::Book,
                ExternalId::new("isbn", "9780441478125").unwrap(),
            ),
            &http(),
        )
        .await
        .unwrap();

    assert_eq!(book.id.as_str(), "openlibrary-work-OL27448W");
    assert_eq!(
        book.titles.entries()[0].value(),
        "The Left Hand of Darkness"
    );
    assert_eq!(book.contributors.len(), 1);
    assert_eq!(book.contributors[0].person.name, "Ursula K. Le Guin");
    assert_eq!(book.contributors[0].role, CreditRole::Author);
    assert_eq!(book.editions.len(), 1);
    assert_eq!(
        book.editions[0].id.as_str(),
        "openlibrary-edition-OL5071201M"
    );
    assert_eq!(book.editions[0].isbn_10.as_str(), "0441478123");
    assert_eq!(book.editions[0].isbn_13.as_str(), "9780441478125");
    assert_eq!(book.editions[0].publisher, "Ace Books");
    assert_eq!(book.editions[0].assets.len(), 1);
    assert_eq!(
        book.editions[0].assets[0].source_path.as_str(),
        format!("{}/covers/id/8231856-L.jpg", server.uri())
    );
}

#[tokio::test]
async fn generic_provider_returns_typed_book_documents() {
    let server = MockServer::start().await;
    mount_fetch_chain(&server).await;
    let provider = OpenLibraryProvider::new(fixture_config(&server)).unwrap();

    let document = provider
        .fetch(
            FetchRequest::new(
                MediaKind::Book,
                ExternalId::new("isbn", "9780441478125").unwrap(),
            ),
            &http(),
        )
        .await
        .unwrap();

    assert!(matches!(document, MetadataDocument::Book(_)));
}

#[tokio::test]
async fn rate_limits_and_malformed_json_remain_structured() {
    let rate_server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(429))
        .mount(&rate_server)
        .await;
    let provider = OpenLibraryProvider::new(fixture_config(&rate_server)).unwrap();
    let direct = provider
        .search_book(SearchRequest::book("Example", None).unwrap(), &http())
        .await
        .unwrap_err();
    assert!(matches!(direct, OpenLibraryError::RateLimited));
    assert_eq!(direct.code(), "rate_limited");

    let generic = provider
        .search(SearchRequest::book("Example", None).unwrap(), &http())
        .await
        .unwrap_err();
    assert!(matches!(generic, ProviderError::Transport(_)));

    let malformed_server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_string("{"))
        .mount(&malformed_server)
        .await;
    let provider = OpenLibraryProvider::new(fixture_config(&malformed_server)).unwrap();
    let error = provider
        .search_book(SearchRequest::book("Example", None).unwrap(), &http())
        .await
        .unwrap_err();
    assert!(matches!(error, OpenLibraryError::MalformedResponse(_)));
}
