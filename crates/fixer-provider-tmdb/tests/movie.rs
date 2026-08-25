use fixer_core::{Candidate, FetchRequest, MediaKind, SearchRequest};
use fixer_http::{HttpConfig, ReqwestHttpClient};
use fixer_provider_tmdb::{TmdbConfig, TmdbError, TmdbProvider};
use std::time::Duration;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{header, method, path, query_param},
};

fn provider(server: &MockServer) -> TmdbProvider {
    TmdbProvider::new(
        TmdbConfig::new("test-token")
            .unwrap()
            .with_base_url(server.uri())
            .unwrap(),
    )
    .unwrap()
}
fn http() -> ReqwestHttpClient {
    ReqwestHttpClient::new(HttpConfig::default()).unwrap()
}
fn search_request() -> SearchRequest {
    SearchRequest::movie("花样年华", Some(2000))
        .unwrap()
        .with_locales(vec!["zh-CN".parse().unwrap()])
}

#[test]
fn token_is_redacted_from_debug_output() {
    let config = TmdbConfig::new("super-secret-token").unwrap();
    assert!(!format!("{config:?}").contains("super-secret-token"));
    let provider = TmdbProvider::new(config).unwrap();
    assert!(!format!("{provider:?}").contains("super-secret-token"));
}

#[tokio::test]
async fn search_sends_auth_query_year_and_requested_locale() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/3/search/movie"))
        .and(header("authorization", "Bearer test-token"))
        .and(query_param("query", "花样年华"))
        .and(query_param("primary_release_year", "2000"))
        .and(query_param("language", "zh-CN"))
        .and(query_param("include_adult", "false"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            include_str!("fixtures/search_movie.json"),
            "application/json",
        ))
        .mount(&server)
        .await;
    let candidates = provider(&server)
        .search_movie(search_request(), &http())
        .await
        .unwrap();
    let Candidate::Movie(movie) = &candidates[0] else {
        panic!("expected movie candidate");
    };
    assert_eq!(movie.external_id.namespace, "tmdb");
    assert_eq!(movie.external_id.value, "843");
    assert_eq!(movie.title, "花样年华");
    assert_eq!(movie.year, Some(2000));
}

#[tokio::test]
async fn fetch_maps_locale_original_language_date_credits_artwork_and_rating() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/3/movie/843"))
        .and(header("authorization", "Bearer test-token"))
        .and(query_param("language", "zh-CN"))
        .and(query_param(
            "append_to_response",
            "credits,images,external_ids",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            include_str!("fixtures/movie_details.json"),
            "application/json",
        ))
        .mount(&server)
        .await;
    let request = FetchRequest::new(
        MediaKind::Movie,
        fixer_core::ExternalId::new("tmdb", "843").unwrap(),
    )
    .with_locales(vec!["zh-CN".parse().unwrap()]);
    let movie = provider(&server)
        .fetch_movie(request, &http())
        .await
        .unwrap();
    assert!(movie.titles.entries().iter().any(|entry| {
        entry
            .language()
            .is_some_and(|language| language.to_string() == "zh-CN")
    }));
    assert!(movie.titles.entries().iter().any(|entry| {
        entry
            .language()
            .is_some_and(|language| language.to_string() == "zh")
    }));
    assert!(movie.releases.iter().any(
        |release| release.release_date.month == Some(9) && release.release_date.day == Some(29)
    ));
    assert_eq!(movie.credits.len(), 4);
    assert!(movie.artwork.len() >= 4);
    assert_eq!(movie.ratings[0].system, "tmdb");
}

#[tokio::test]
async fn status_malformed_empty_and_timeout_failures_are_distinct() {
    for (status, expected) in [(401, "unauthorized"), (429, "rate_limited")] {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/3/search/movie"))
            .respond_with(ResponseTemplate::new(status))
            .mount(&server)
            .await;
        let error = provider(&server)
            .search_movie(search_request(), &http())
            .await
            .unwrap_err();
        assert_eq!(error.code(), expected);
    }
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/3/movie/843"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;
    let request = FetchRequest::new(
        MediaKind::Movie,
        fixer_core::ExternalId::new("tmdb", "843").unwrap(),
    );
    assert!(matches!(
        provider(&server)
            .fetch_movie(request, &http())
            .await
            .unwrap_err(),
        TmdbError::NotFound
    ));

    let malformed = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/3/search/movie"))
        .respond_with(ResponseTemplate::new(200).set_body_string("not-json"))
        .mount(&malformed)
        .await;
    assert!(matches!(
        provider(&malformed)
            .search_movie(search_request(), &http())
            .await
            .unwrap_err(),
        TmdbError::MalformedResponse(_)
    ));

    let empty = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/3/search/movie"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            r#"{"page":1,"results":[],"total_pages":0,"total_results":0}"#,
            "application/json",
        ))
        .mount(&empty)
        .await;
    assert!(matches!(
        provider(&empty)
            .search_movie(search_request(), &http())
            .await
            .unwrap_err(),
        TmdbError::EmptyResults
    ));

    let slow = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/3/search/movie"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_delay(Duration::from_millis(150))
                .set_body_raw(
                    include_str!("fixtures/search_movie.json"),
                    "application/json",
                ),
        )
        .mount(&slow)
        .await;
    let timed_http =
        ReqwestHttpClient::new(HttpConfig::default().with_timeout(Duration::from_millis(20)))
            .unwrap();
    assert!(matches!(
        provider(&slow)
            .search_movie(search_request(), &timed_http)
            .await
            .unwrap_err(),
        TmdbError::Timeout
    ));
}

#[tokio::test]
#[ignore = "requires TMDB_API_TOKEN and live network access"]
async fn live_tmdb_smoke() {
    let provider = TmdbProvider::new(TmdbConfig::from_env().unwrap()).unwrap();
    let results = provider
        .search_movie(search_request(), &http())
        .await
        .unwrap();
    assert!(!results.is_empty());
}
