use fixer_core::{ExternalId, OrderingScheme};
use fixer_http::{HttpConfig, ReqwestHttpClient};
use fixer_provider_local::LocalProvider;
use fixer_provider_tmdb::{TmdbConfig, TmdbProvider};
use fixer_sdk::Fixer;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path},
};

#[tokio::test]
async fn local_episode_facts_merge_with_tmdb_series_and_episode_metadata() {
    let root = tempfile::tempdir().unwrap();
    let season = root.path().join("Example Show").join("Season 01");
    std::fs::create_dir_all(&season).unwrap();
    std::fs::write(
        season.join("Example.Show.S01E02.{imdb-tt0944947}.{tmdb-1399}.mkv"),
        [],
    )
    .unwrap();
    std::fs::write(
        season.join("Example.Show.S01E02.{imdb-tt0944947}.{tmdb-1399}.tags.xml"),
        r#"<Tags><Tag>
            <Simple><Name>TVSHOW</Name><String>本地剧名</String></Simple>
            <Simple><Name>TITLE</Name><String>Local Episode Title</String></Simple>
            <Simple><Name>SEASON</Name><String>1</String></Simple>
            <Simple><Name>EPISODE</Name><String>2</String></Simple>
            <Simple><Name>TMDBID</Name><String>1399</String></Simple>
        </Tag></Tags>"#,
    )
    .unwrap();
    let (local, warnings) = LocalProvider::from_scan(root.path()).unwrap();
    assert!(warnings.is_empty());

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/3/search/tv"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            r#"{
              "page": 1,
              "results": [
                {"id": 1399, "name": "权力的游戏", "first_air_date": "2011-04-17"},
                {"id": 9999, "name": "Unrelated Show", "first_air_date": "2011-01-01"}
              ],
              "total_pages": 1,
              "total_results": 2
            }"#,
            "application/json",
        ))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/3/tv/1399"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            include_str!("../../fixer-provider-tmdb/tests/fixtures/tv_details.json"),
            "application/json",
        ))
        .mount(&server)
        .await;
    for (season, fixture) in [
        (
            0,
            include_str!("../../fixer-provider-tmdb/tests/fixtures/tv_season_0.json"),
        ),
        (
            1,
            include_str!("../../fixer-provider-tmdb/tests/fixtures/tv_season_1.json"),
        ),
    ] {
        Mock::given(method("GET"))
            .and(path(format!("/3/tv/1399/season/{season}")))
            .respond_with(ResponseTemplate::new(200).set_body_raw(fixture, "application/json"))
            .mount(&server)
            .await;
    }

    let tmdb = TmdbProvider::new(
        TmdbConfig::new("test-token")
            .unwrap()
            .with_base_url(server.uri())
            .unwrap()
            .with_image_base_url(format!("{}/images/", server.uri()))
            .unwrap(),
    )
    .unwrap();
    let fixer = Fixer::builder()
        .provider(local)
        .provider(tmdb)
        .preferred_languages(["zh-CN", "en"])
        .unwrap()
        .http_client(ReqwestHttpClient::new(HttpConfig::default()).unwrap())
        .build()
        .unwrap();

    let resolved = fixer
        .television("Example Show")
        .year(2011)
        .external_id(ExternalId::new("imdb", "tt0944947").unwrap())
        .external_id(ExternalId::new("tmdb", "1399").unwrap())
        .ordering(OrderingScheme::Aired)
        .resolve()
        .await
        .unwrap();

    assert_eq!(resolved.value.ordering, OrderingScheme::Aired);
    assert_eq!(resolved.value.seasons.len(), 2);
    let episode = &resolved
        .value
        .seasons
        .iter()
        .find(|season| season.number == 1)
        .unwrap()
        .episodes[0];
    assert_eq!(episode.titles.entries()[0].value(), "Local Episode Title");
    assert_eq!(episode.runtime.unwrap().as_seconds(), 55 * 60);
    assert_eq!(episode.credits.len(), 2);
    assert!(!episode.artwork.is_empty());
    assert!(!resolved.value.artwork.is_empty());
    assert!(
        !resolved
            .provenance
            .sources_for("series.seasons.1.episodes.2.titles")
            .is_empty()
    );
    assert!(
        !resolved
            .provenance
            .sources_for("series.seasons.1.episodes.2.runtime")
            .is_empty()
    );
    let requests = server.received_requests().await.unwrap();
    assert!(
        !requests
            .iter()
            .any(|request| request.url.path() == "/3/tv/9999")
    );

    let error = fixer
        .television("Example Show")
        .external_id(ExternalId::new("tmdb", "1399").unwrap())
        .ordering(OrderingScheme::Absolute)
        .resolve()
        .await
        .unwrap_err();
    assert!(matches!(
        error,
        fixer_sdk::SdkError::OrderingUnavailable {
            requested: OrderingScheme::Absolute
        }
    ));
}
