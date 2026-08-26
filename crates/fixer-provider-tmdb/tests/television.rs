use fixer_core::{Candidate, FetchRequest, MediaKind, OrderingScheme, Provider, SearchRequest};
use fixer_http::{HttpConfig, ReqwestHttpClient};
use fixer_provider_tmdb::{TmdbConfig, TmdbProvider};
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{header, method, path, query_param},
};

fn provider(server: &MockServer) -> TmdbProvider {
    TmdbProvider::new(
        TmdbConfig::new("test-token")
            .unwrap()
            .with_base_url(server.uri())
            .unwrap()
            .with_image_base_url(format!("{}/images/", server.uri()))
            .unwrap(),
    )
    .unwrap()
}

fn http() -> ReqwestHttpClient {
    ReqwestHttpClient::new(HttpConfig::default()).unwrap()
}

#[tokio::test]
async fn searches_tv_with_year_locale_and_typed_candidate() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/3/search/tv"))
        .and(header("authorization", "Bearer test-token"))
        .and(query_param("query", "权力的游戏"))
        .and(query_param("first_air_date_year", "2011"))
        .and(query_param("language", "zh-CN"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_raw(include_str!("fixtures/tv_search.json"), "application/json"),
        )
        .mount(&server)
        .await;

    let request = SearchRequest::television("权力的游戏", Some(2011))
        .unwrap()
        .with_locales(vec!["zh-CN".parse().unwrap()]);
    let candidates = provider(&server)
        .search_television(request, &http())
        .await
        .unwrap();
    let Candidate::Television(candidate) = &candidates[0] else {
        panic!("expected television candidate");
    };
    assert_eq!(candidate.external_id.namespace, "tmdb");
    assert_eq!(candidate.external_id.value, "1399");
    assert_eq!(candidate.year, Some(2011));
}

#[tokio::test]
async fn fetches_series_specials_seasons_episodes_credits_and_artwork() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/3/tv/1399"))
        .and(header("authorization", "Bearer test-token"))
        .and(query_param("language", "zh-CN"))
        .and(query_param("append_to_response", "images,external_ids"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_raw(include_str!("fixtures/tv_details.json"), "application/json"),
        )
        .mount(&server)
        .await;
    for (season, fixture) in [
        (0, include_str!("fixtures/tv_season_0.json")),
        (1, include_str!("fixtures/tv_season_1.json")),
    ] {
        Mock::given(method("GET"))
            .and(path(format!("/3/tv/1399/season/{season}")))
            .and(query_param("language", "zh-CN"))
            .respond_with(ResponseTemplate::new(200).set_body_raw(fixture, "application/json"))
            .mount(&server)
            .await;
    }

    let request = FetchRequest::new(
        MediaKind::Television,
        fixer_core::ExternalId::new("tmdb", "1399").unwrap(),
    )
    .with_locales(vec!["zh-CN".parse().unwrap()]);
    let series = provider(&server)
        .fetch_television(request, &http())
        .await
        .unwrap();

    assert_eq!(series.ordering, OrderingScheme::Aired);
    assert_eq!(series.seasons.len(), 2);
    assert_eq!(series.seasons[0].number, 0);
    assert!(!series.seasons[0].artwork.is_empty());
    let episode = &series.seasons[1].episodes[0];
    assert_eq!(episode.sequence.season, Some(1));
    assert_eq!(episode.sequence.episode, 2);
    assert_eq!(episode.runtime.unwrap().as_seconds(), 55 * 60);
    assert_eq!(episode.credits.len(), 2);
    assert!(!episode.artwork.is_empty());
    assert!(series.artwork.len() >= 4);
    assert!(
        provider(&server)
            .descriptor()
            .supports(MediaKind::Television)
    );
}
