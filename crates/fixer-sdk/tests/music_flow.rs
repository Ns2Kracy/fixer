use fixer_core::{Candidate, OutputOperation};
use fixer_provider_musicbrainz::{MusicBrainzConfig, MusicBrainzProvider};
use fixer_sdk::Fixer;
use fixer_writer_local::MusicWriter;
use std::time::Duration;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path},
};

async fn fixture_provider(server: &MockServer) -> MusicBrainzProvider {
    Mock::given(method("GET"))
        .and(path("/ws/2/release-group/"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            include_str!("../../fixer-provider-musicbrainz/tests/fixtures/search.json"),
            "application/json",
        ))
        .mount(server)
        .await;
    Mock::given(method("GET"))
        .and(path(
            "/ws/2/release-group/1f2a3b4c-1111-2222-3333-444455556666",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            include_str!("../../fixer-provider-musicbrainz/tests/fixtures/release_group.json"),
            "application/json",
        ))
        .mount(server)
        .await;
    Mock::given(method("GET"))
        .and(path("/ws/2/release/aaaaaaaa-1111-2222-3333-444455556666"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            include_str!("../../fixer-provider-musicbrainz/tests/fixtures/release.json"),
            "application/json",
        ))
        .mount(server)
        .await;
    MusicBrainzProvider::new(
        MusicBrainzConfig::default()
            .with_base_url(format!("{}/ws/2/", server.uri()))
            .unwrap()
            .with_minimum_request_interval(Duration::from_millis(1))
            .unwrap(),
    )
    .unwrap()
}

#[tokio::test]
async fn typed_album_search_selection_fetch_and_output_planning_stay_typed() {
    let server = MockServer::start().await;
    let fixer = Fixer::builder()
        .provider(fixture_provider(&server).await)
        .build()
        .unwrap();

    let search = fixer
        .music("Kind of Blue")
        .year(1959)
        .search()
        .await
        .unwrap();
    assert_eq!(search.candidates().len(), 1);
    assert!(matches!(search.candidates()[0], Candidate::Music(_)));

    let resolved = search.select(0).unwrap().fetch_selected().await.unwrap();
    assert_eq!(resolved.value.artist.name, "Miles Davis");
    assert_eq!(resolved.value.releases[0].discs.len(), 2);
    assert_eq!(
        resolved.provenance.sources_for("music.release_group")[0]
            .provider
            .as_str(),
        "musicbrainz"
    );
    assert_eq!(
        resolved.provenance.sources_for("music.tracks")[0]
            .provider
            .as_str(),
        "musicbrainz"
    );

    let plan = MusicWriter::default()
        .plan_resolved(&resolved, "library/Miles Davis/Kind of Blue")
        .unwrap();
    assert_eq!(plan.operations().len(), 2);
    assert!(
        plan.operations()
            .iter()
            .all(|operation| matches!(operation, OutputOperation::WriteBytes { .. }))
    );
}

#[tokio::test]
async fn typed_music_resolve_fetches_the_ranked_top_candidate() {
    let server = MockServer::start().await;
    let fixer = Fixer::builder()
        .provider(fixture_provider(&server).await)
        .build()
        .unwrap();

    let resolved = fixer
        .music("Kind of Blue")
        .year(1959)
        .resolve()
        .await
        .unwrap();

    assert_eq!(
        resolved.value.id.as_str(),
        "musicbrainz-release-group-1f2a3b4c-1111-2222-3333-444455556666"
    );
    assert_eq!(resolved.value.releases[0].discs[0].tracks.len(), 2);
}
