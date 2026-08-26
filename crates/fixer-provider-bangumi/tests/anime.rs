use fixer_core::{
    AnimeEpisodeClass, Candidate, FetchRequest, HttpError, MediaKind, Provider, ProviderError,
    SearchRequest,
};
use fixer_http::{HttpConfig, ReqwestHttpClient};
use fixer_provider_bangumi::{BangumiConfig, BangumiError, BangumiProvider};
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{body_json, header, method, path, query_param},
};

fn http() -> ReqwestHttpClient {
    ReqwestHttpClient::new(HttpConfig::default()).unwrap()
}

#[tokio::test]
async fn search_uses_v0_anime_filter_endpoint_override_and_identifiable_user_agent() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v0/search/subjects"))
        .and(query_param("limit", "25"))
        .and(query_param("offset", "0"))
        .and(header(
            "user-agent",
            "ns2kracy/fixer/0.1.0 (https://github.com/ns2kracy/fixer)",
        ))
        .and(header("content-type", "application/json"))
        .and(body_json(serde_json::json!({
            "keyword": "葬送のフリーレン",
            "sort": "match",
            "filter": {
                "type": [2],
                "air_date": [">=2023-01-01", "<2024-01-01"]
            }
        })))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            include_str!("fixtures/anime_search.json"),
            "application/json",
        ))
        .mount(&server)
        .await;

    let provider = BangumiProvider::new(
        BangumiConfig::default()
            .with_base_url(server.uri())
            .unwrap(),
    )
    .unwrap();
    let candidates = provider
        .search(
            SearchRequest::anime("葬送のフリーレン", Some(2023)).unwrap(),
            &http(),
        )
        .await
        .unwrap();

    let Candidate::Anime(candidate) = &candidates[0] else {
        panic!("expected anime candidate");
    };
    assert_eq!(candidate.provider.as_str(), "bangumi");
    assert_eq!(candidate.external_id.namespace, "bangumi");
    assert_eq!(candidate.external_id.value, "400602");
    assert_eq!(candidate.year, Some(2023));
}

#[tokio::test]
async fn fetch_preserves_localized_titles_specials_and_aired_numbering() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v0/subjects/400602"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            include_str!("fixtures/anime_details.json"),
            "application/json",
        ))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/v0/episodes"))
        .and(query_param("subject_id", "400602"))
        .and(query_param("limit", "200"))
        .and(query_param("offset", "0"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            include_str!("fixtures/anime_episodes.json"),
            "application/json",
        ))
        .mount(&server)
        .await;

    let provider = BangumiProvider::new(
        BangumiConfig::default()
            .with_base_url(server.uri())
            .unwrap(),
    )
    .unwrap();
    let anime = provider
        .fetch_anime(
            FetchRequest::new(
                MediaKind::Anime,
                fixer_core::ExternalId::new("bangumi", "400602").unwrap(),
            ),
            &http(),
        )
        .await
        .unwrap();

    let locales = anime
        .titles
        .entries()
        .iter()
        .filter_map(|entry| entry.language().map(ToString::to_string))
        .collect::<Vec<_>>();
    assert!(locales.contains(&"ja".to_owned()));
    assert!(locales.contains(&"zh-Hans".to_owned()));
    assert!(locales.contains(&"zh-Hant".to_owned()));
    assert!(locales.contains(&"en".to_owned()));
    assert_eq!(anime.cours.len(), 1);
    assert_eq!(anime.cours[0].episodes[0].class, AnimeEpisodeClass::Regular);
    assert_eq!(anime.cours[0].episodes[0].aired_number, Some(1));
    assert_eq!(anime.cours[0].episodes[0].absolute_number, None);
    assert_eq!(anime.cours[0].episodes[2].class, AnimeEpisodeClass::Special);
    assert_eq!(anime.cours[0].episodes[2].aired_number, Some(1));
}

#[tokio::test]
async fn subject_platform_distinguishes_ova_and_ona_main_episodes() {
    for (platform, expected) in [
        ("OVA", AnimeEpisodeClass::Ova),
        ("Web", AnimeEpisodeClass::Ona),
    ] {
        let server = MockServer::start().await;
        let details = serde_json::json!({
            "id": 42,
            "type": 2,
            "name": "Example",
            "name_cn": "示例",
            "summary": "",
            "date": "2024-01-01",
            "platform": platform,
            "eps": 1,
            "infobox": []
        });
        let episodes = serde_json::json!({
            "total": 1,
            "limit": 200,
            "offset": 0,
            "data": [{
                "id": 4201,
                "type": 0,
                "name": "Episode",
                "name_cn": "章节",
                "sort": 1.0,
                "ep": 1.0,
                "airdate": "2024-01-01",
                "comment": 0,
                "duration": "00:20:00",
                "desc": "",
                "disc": 0,
                "subject_id": 42
            }]
        });
        Mock::given(method("GET"))
            .and(path("/v0/subjects/42"))
            .respond_with(ResponseTemplate::new(200).set_body_json(details))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/v0/episodes"))
            .respond_with(ResponseTemplate::new(200).set_body_json(episodes))
            .mount(&server)
            .await;
        let provider = BangumiProvider::new(
            BangumiConfig::default()
                .with_base_url(server.uri())
                .unwrap(),
        )
        .unwrap();
        let anime = provider
            .fetch_anime(
                FetchRequest::new(
                    MediaKind::Anime,
                    fixer_core::ExternalId::new("bangumi", "42").unwrap(),
                ),
                &http(),
            )
            .await
            .unwrap();
        assert_eq!(anime.cours[0].episodes[0].class, expected);
    }
}

#[tokio::test]
async fn rate_limits_remain_structured_provider_failures() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v0/search/subjects"))
        .respond_with(ResponseTemplate::new(429))
        .mount(&server)
        .await;
    let provider = BangumiProvider::new(
        BangumiConfig::default()
            .with_base_url(server.uri())
            .unwrap(),
    )
    .unwrap();

    let direct = provider
        .search_anime(SearchRequest::anime("Example", None).unwrap(), &http())
        .await
        .unwrap_err();
    assert!(matches!(direct, BangumiError::RateLimited));
    assert_eq!(direct.code(), "rate_limited");

    let generic = provider
        .search(SearchRequest::anime("Example", None).unwrap(), &http())
        .await
        .unwrap_err();
    assert!(matches!(generic, ProviderError::Transport(_)));
}

#[test]
fn offline_http_errors_are_structured() {
    let error = BangumiError::from_http(HttpError::Offline);
    assert_eq!(error.code(), "offline");
}
