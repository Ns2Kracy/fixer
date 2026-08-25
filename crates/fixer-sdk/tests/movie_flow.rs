use fixer_core::{
    ExternalId, MetadataDocument, Movie, MovieRelease, ProviderId, ReleaseDate, ReleaseId, WorkId,
};
use fixer_provider_local::{LocalProvider, parse_json};
use fixer_sdk::{Fixer, FixtureDocument, FixtureProvider, SdkError};
use std::time::{Duration, Instant};

fn movie(id: &str, title: &str, year: u16, summary: Option<(&str, &str)>) -> Movie {
    let mut titles = fixer_core::LocalizedValue::new();
    titles.insert("zh-CN", title.to_owned()).unwrap();
    let mut movie = Movie::new(WorkId::new(id).unwrap(), titles);
    movie.releases.push(MovieRelease::new(
        ReleaseId::new(format!("{id}-release")).unwrap(),
        ReleaseDate::year(year).unwrap(),
    ));
    if let Some((language, summary)) = summary {
        movie
            .summaries
            .insert(language, summary.to_owned())
            .unwrap();
    }
    movie
}

fn fixture(
    provider: &str,
    id: &str,
    title: &str,
    year: u16,
    summary: Option<(&str, &str)>,
) -> FixtureDocument {
    FixtureDocument::new(
        ExternalId::new(provider, id).unwrap(),
        MetadataDocument::Movie(movie(id, title, year, summary)),
    )
}

#[tokio::test]
async fn resolves_a_movie_through_the_ergonomic_api() {
    let local = FixtureProvider::new(
        ProviderId::new("fixture.local").unwrap(),
        [fixture(
            "fixture.local",
            "movie-1",
            "花样年华",
            2000,
            Some(("zh-CN", "本地简介")),
        )],
    )
    .unwrap()
    .with_search_delay(Duration::from_millis(100));
    let remote = FixtureProvider::new(
        ProviderId::new("fixture.remote").unwrap(),
        [fixture(
            "fixture.remote",
            "movie-2",
            "花样年华",
            2000,
            Some(("en", "Remote summary")),
        )],
    )
    .unwrap()
    .with_search_delay(Duration::from_millis(100));

    let fixer = Fixer::builder()
        .provider(local)
        .provider(remote)
        .preferred_languages(["zh-CN", "zh-TW", "en"])
        .unwrap()
        .build()
        .unwrap();

    let started = Instant::now();
    let outcome = fixer.movie("花样年华").year(2000).resolve().await.unwrap();
    assert!(
        started.elapsed() < Duration::from_millis(180),
        "providers were searched serially"
    );
    assert_eq!(outcome.value().release_year(), Some(2000));
    assert_eq!(outcome.value().summaries.entries().len(), 2);
    assert!(!outcome.provenance.sources_for("movie.titles").is_empty());
    assert!(
        outcome
            .warnings
            .iter()
            .any(|warning| warning.code == "ambiguous_candidates")
    );
}

#[tokio::test]
async fn lower_level_search_select_and_fetch_are_available() {
    let provider = FixtureProvider::new(
        ProviderId::new("fixture").unwrap(),
        [fixture("fixture", "movie-1", "Movie", 2000, None)],
    )
    .unwrap();
    let fixer = Fixer::builder().provider(provider).build().unwrap();

    let search = fixer.movie("Movie").year(2000).search().await.unwrap();
    assert_eq!(search.candidates().len(), 1);
    let selected = search.select(0).unwrap();
    let outcome = selected.fetch_selected().await.unwrap();
    assert_eq!(outcome.value().release_year(), Some(2000));
}

#[tokio::test]
async fn resolves_through_the_local_metadata_provider() {
    let provider = LocalProvider::from_documents([parse_json(include_str!(
        "../../fixer-provider-local/tests/fixtures/movie.json"
    ))
    .unwrap()])
    .unwrap();
    let fixer = Fixer::builder()
        .provider(provider)
        .offline()
        .build()
        .unwrap();

    let outcome = fixer.movie("花样年华").year(2000).resolve().await.unwrap();
    assert_eq!(outcome.value().release_year(), Some(2000));
    assert_eq!(outcome.value().titles.entries().len(), 2);
}

#[tokio::test]
async fn local_and_tmdb_merge_complementary_movie_fields() {
    use fixer_http::{HttpConfig, ReqwestHttpClient};
    use fixer_provider_local::{LocalProvider, parse_json};
    use fixer_provider_tmdb::{TmdbConfig, TmdbProvider};
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{method, path},
    };

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/3/search/movie"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            include_str!("../../fixer-provider-tmdb/tests/fixtures/search_movie.json"),
            "application/json",
        ))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/3/movie/843"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            include_str!("../../fixer-provider-tmdb/tests/fixtures/movie_details.json"),
            "application/json",
        ))
        .mount(&server)
        .await;
    let mut local_movie = parse_json(include_str!(
        "../../fixer-provider-local/tests/fixtures/movie.json"
    ))
    .unwrap();
    local_movie
        .summaries
        .insert("zh-CN", "本地中文简介。".to_owned())
        .unwrap();
    let local = LocalProvider::from_documents([local_movie]).unwrap();
    let tmdb = TmdbProvider::new(
        TmdbConfig::new("test-token")
            .unwrap()
            .with_base_url(server.uri())
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
    let resolved = fixer.movie("花样年华").year(2000).resolve().await.unwrap();
    assert!(
        resolved
            .value
            .summaries
            .entries()
            .iter()
            .any(|entry| entry.value() == "本地中文简介。")
    );
    assert!(!resolved.value.credits.is_empty());
    assert!(!resolved.value.artwork.is_empty());
    assert!(
        resolved
            .value
            .releases
            .iter()
            .any(|release| release.release_date.month == Some(9))
    );
    assert!(
        resolved
            .provenance
            .sources_for("movie.summaries")
            .iter()
            .any(|source| source.provider.as_str() == "local")
    );
    assert!(
        resolved
            .provenance
            .sources_for("movie.credits")
            .iter()
            .any(|source| source.provider.as_str() == "tmdb")
    );
}

#[test]
fn builder_rejects_invalid_configuration() {
    assert!(matches!(
        Fixer::builder().build(),
        Err(SdkError::NoProviders)
    ));
    assert!(Fixer::builder().preferred_languages(["not_a_tag"]).is_err());

    let first = FixtureProvider::new(
        ProviderId::new("duplicate").unwrap(),
        [fixture("duplicate", "movie-1", "Movie", 2000, None)],
    )
    .unwrap();
    let second = FixtureProvider::new(
        ProviderId::new("duplicate").unwrap(),
        [fixture("duplicate", "movie-2", "Other", 2001, None)],
    )
    .unwrap();
    assert!(matches!(
        Fixer::builder().provider(first).provider(second).build(),
        Err(SdkError::DuplicateProvider(id)) if id.as_str() == "duplicate"
    ));
}
