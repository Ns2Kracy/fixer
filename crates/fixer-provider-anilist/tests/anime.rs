use fixer_core::{
    ArtworkKind, Candidate, ExternalId, FetchRequest, MediaKind, Provider, ProviderError,
    SearchRequest,
};
use fixer_http::{HttpConfig, ReqwestHttpClient};
use fixer_provider_anilist::{AniListConfig, AniListError, AniListProvider};
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{body_partial_json, header, method, path},
};

fn http() -> ReqwestHttpClient {
    ReqwestHttpClient::new(HttpConfig::default()).unwrap()
}

#[tokio::test]
async fn search_posts_anime_variables_to_the_overridden_endpoint_with_optional_auth() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(header("accept", "application/json"))
        .and(header("content-type", "application/json"))
        .and(header("authorization", "Bearer fixture-token"))
        .and(body_partial_json(serde_json::json!({
            "variables": { "search": "Sousou no Frieren" }
        })))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_raw(include_str!("fixtures/search.json"), "application/json"),
        )
        .mount(&server)
        .await;

    let provider = AniListProvider::new(
        AniListConfig::default()
            .with_endpoint(format!("{}/graphql", server.uri()))
            .unwrap()
            .with_access_token("fixture-token")
            .unwrap(),
    )
    .unwrap();
    let candidates = provider
        .search_anime(
            SearchRequest::anime("Sousou no Frieren", Some(2023)).unwrap(),
            &http(),
        )
        .await
        .unwrap();

    let Candidate::Anime(candidate) = &candidates[0] else {
        panic!("expected anime candidate");
    };
    assert_eq!(candidate.provider.as_str(), "anilist");
    assert_eq!(candidate.external_id.namespace, "anilist");
    assert_eq!(candidate.external_id.value, "154587");
    assert_eq!(candidate.title, "Frieren: Beyond Journey's End");
    assert_eq!(candidate.year, Some(2023));
}

#[tokio::test]
async fn fetch_maps_alternate_titles_summary_cover_and_banner_without_fake_episodes() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(body_partial_json(serde_json::json!({
            "variables": { "id": 154587 }
        })))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_raw(include_str!("fixtures/fetch.json"), "application/json"),
        )
        .mount(&server)
        .await;

    let provider = AniListProvider::new(
        AniListConfig::default()
            .with_endpoint(format!("{}/graphql", server.uri()))
            .unwrap(),
    )
    .unwrap();
    let anime = provider
        .fetch_anime(
            FetchRequest::new(
                MediaKind::Anime,
                ExternalId::new("anilist", "154587").unwrap(),
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
    assert!(locales.contains(&"ja-Latn".to_owned()));
    assert!(locales.contains(&"en".to_owned()));
    assert!(locales.contains(&"ja".to_owned()));
    assert_eq!(anime.titles.entries().len(), 4);
    assert_eq!(
        anime.summaries.entries()[0].value(),
        "An elf mage learns what a short human life means."
    );
    assert_eq!(anime.artwork.len(), 2);
    assert_eq!(anime.artwork[0].kind, ArtworkKind::Cover);
    assert_eq!(anime.artwork[1].kind, ArtworkKind::Banner);
    assert!(anime.cours.is_empty());
}

#[tokio::test]
async fn graphql_errors_remain_structured_across_direct_and_generic_apis() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "data": null,
            "errors": [{ "message": "Too many requests." }]
        })))
        .mount(&server)
        .await;
    let provider = AniListProvider::new(
        AniListConfig::default()
            .with_endpoint(server.uri())
            .unwrap(),
    )
    .unwrap();

    let direct = provider
        .search_anime(SearchRequest::anime("Example", None).unwrap(), &http())
        .await
        .unwrap_err();
    assert!(
        matches!(direct, AniListError::GraphQl(ref message) if message == "Too many requests.")
    );
    assert_eq!(direct.code(), "graphql");

    let generic = provider
        .search(SearchRequest::anime("Example", None).unwrap(), &http())
        .await
        .unwrap_err();
    assert!(
        matches!(generic, ProviderError::InvalidResponse(message) if message == "Too many requests.")
    );
}
