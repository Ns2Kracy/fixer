use fixer_core::{
    BoxFuture, Candidate, ExternalId, FetchRequest, HttpClient, HttpError, HttpRequest,
    HttpResponse, MediaKind, Provider, ProviderError, SearchRequest,
};
use fixer_http::{HttpConfig, ReqwestHttpClient};
use fixer_provider_musicbrainz::{MusicBrainzConfig, MusicBrainzError, MusicBrainzProvider};
use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{header, method, path, query_param},
};

fn http() -> ReqwestHttpClient {
    ReqwestHttpClient::new(HttpConfig::default()).unwrap()
}

fn fixture_config(server: &MockServer) -> MusicBrainzConfig {
    MusicBrainzConfig::default()
        .with_base_url(format!("{}/ws/2/", server.uri()))
        .unwrap()
        .with_minimum_request_interval(Duration::from_millis(1))
        .unwrap()
}

#[tokio::test]
async fn search_uses_release_group_query_json_and_identifiable_user_agent() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/ws/2/release-group/"))
        .and(query_param(
            "query",
            "releasegroup:\"Kind of Blue\" AND firstreleasedate:1959",
        ))
        .and(query_param("fmt", "json"))
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
    let provider = MusicBrainzProvider::new(fixture_config(&server)).unwrap();

    let candidates = provider
        .search_music(
            SearchRequest::music("Kind of Blue", Some(1959)).unwrap(),
            &http(),
        )
        .await
        .unwrap();

    let Candidate::Music(candidate) = &candidates[0] else {
        panic!("expected music candidate");
    };
    assert_eq!(candidate.provider.as_str(), "musicbrainz");
    assert_eq!(candidate.external_id.namespace, "musicbrainz-release-group");
    assert_eq!(
        candidate.external_id.value,
        "1f2a3b4c-1111-2222-3333-444455556666"
    );
    assert_eq!(candidate.title, "Kind of Blue");
    assert_eq!(candidate.year, Some(1959));
}

#[tokio::test]
async fn fetch_resolves_artist_group_earliest_release_discs_and_recordings() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(
            "/ws/2/release-group/1f2a3b4c-1111-2222-3333-444455556666",
        ))
        .and(query_param("inc", "releases+artist-credits"))
        .and(query_param("fmt", "json"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            include_str!("fixtures/release_group.json"),
            "application/json",
        ))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/ws/2/release/aaaaaaaa-1111-2222-3333-444455556666"))
        .and(query_param("inc", "recordings"))
        .and(query_param("fmt", "json"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_raw(include_str!("fixtures/release.json"), "application/json"),
        )
        .mount(&server)
        .await;
    let provider = MusicBrainzProvider::new(fixture_config(&server)).unwrap();

    let group = provider
        .fetch_music(
            FetchRequest::new(
                MediaKind::Music,
                ExternalId::new(
                    "musicbrainz-release-group",
                    "1f2a3b4c-1111-2222-3333-444455556666",
                )
                .unwrap(),
            ),
            &http(),
        )
        .await
        .unwrap();

    assert_eq!(group.artist.name, "Miles Davis");
    assert!(group.artist.id.as_str().contains("561d854a"));
    assert_eq!(group.releases.len(), 1);
    assert!(group.releases[0].id.as_str().contains("aaaaaaaa"));
    assert_eq!(group.releases[0].discs.len(), 2);
    assert_eq!(group.releases[0].discs[0].number, 1);
    assert_eq!(group.releases[0].discs[0].tracks.len(), 2);
    assert_eq!(group.releases[0].discs[0].tracks[0].sequence.disc, 1);
    assert_eq!(group.releases[0].discs[0].tracks[0].sequence.track, 1);
    assert_eq!(
        group.releases[0].discs[0].tracks[0].duration.as_seconds(),
        562
    );
    assert!(
        group.releases[0].discs[0].tracks[0]
            .id
            .as_str()
            .contains("recording-1")
    );
    assert_eq!(group.releases[0].discs[1].tracks[0].sequence.disc, 2);
}

#[tokio::test]
async fn rate_limits_remain_structured_provider_failures() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(429))
        .mount(&server)
        .await;
    let provider = MusicBrainzProvider::new(fixture_config(&server)).unwrap();

    let direct = provider
        .search_music(SearchRequest::music("Example", None).unwrap(), &http())
        .await
        .unwrap_err();
    assert!(matches!(direct, MusicBrainzError::RateLimited));
    assert_eq!(direct.code(), "rate_limited");

    let generic = provider
        .search(SearchRequest::music("Example", None).unwrap(), &http())
        .await
        .unwrap_err();
    assert!(matches!(generic, ProviderError::Transport(_)));
}

#[derive(Clone)]
struct RecordingHttp {
    starts: Arc<Mutex<Vec<Instant>>>,
}

impl HttpClient for RecordingHttp {
    fn execute(&self, _: HttpRequest) -> BoxFuture<'_, Result<HttpResponse, HttpError>> {
        let starts = self.starts.clone();
        Box::pin(async move {
            starts.lock().unwrap().push(Instant::now());
            Ok(HttpResponse::new(200).with_body(include_bytes!("fixtures/search.json").to_vec()))
        })
    }
}

#[tokio::test]
async fn cloned_providers_share_the_request_pacing_gate() {
    let provider = MusicBrainzProvider::new(
        MusicBrainzConfig::default()
            .with_minimum_request_interval(Duration::from_millis(50))
            .unwrap(),
    )
    .unwrap();
    let cloned = provider.clone();
    let starts = Arc::new(Mutex::new(Vec::new()));
    let http = RecordingHttp {
        starts: starts.clone(),
    };

    let (left, right) = tokio::join!(
        provider.search_music(SearchRequest::music("Kind of Blue", None).unwrap(), &http),
        cloned.search_music(SearchRequest::music("Kind of Blue", None).unwrap(), &http),
    );
    left.unwrap();
    right.unwrap();
    let starts = starts.lock().unwrap();
    assert_eq!(starts.len(), 2);
    assert!(starts[1].duration_since(starts[0]) >= Duration::from_millis(40));
    drop(starts);
}
