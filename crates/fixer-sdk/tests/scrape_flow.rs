use fixer_core::{
    ExternalId, MediaKind, MetadataDocument, Movie, MovieRelease, ProviderId, ProviderTarget,
    ReleaseDate, ReleaseId, ScrapeSelection, WorkId,
};
use fixer_sdk::{Fixer, FixtureDocument, FixtureProvider, ScrapedMedia};

fn movie(id: &str, title: &str, year: u16, summary: (&str, &str)) -> Movie {
    let mut titles = fixer_core::LocalizedValue::new();
    titles.insert("zh-CN", title.to_owned()).unwrap();
    let mut movie = Movie::new(WorkId::new(id).unwrap(), titles);
    movie.releases.push(MovieRelease::new(
        ReleaseId::new(format!("{id}-release")).unwrap(),
        ReleaseDate::year(year).unwrap(),
    ));
    movie
        .summaries
        .insert(summary.0, summary.1.to_owned())
        .unwrap();
    movie
}

fn fixture(provider: &str, id: &str, value: Movie) -> FixtureDocument {
    FixtureDocument::new(
        ExternalId::new(provider, id).unwrap(),
        MetadataDocument::Movie(value),
    )
}

fn configured_fixer() -> Fixer {
    let provider = FixtureProvider::new(
        ProviderId::new("fixture.remote").unwrap(),
        [fixture(
            "fixture.remote",
            "movie-remote",
            movie("remote-work", "花样年华", 2000, ("en", "Remote summary")),
        )],
    )
    .unwrap();
    Fixer::builder()
        .provider(provider)
        .offline()
        .build()
        .unwrap()
}

#[tokio::test]
async fn automatic_scrape_selects_and_merges_without_exposing_candidates() {
    let scanned =
        MetadataDocument::Movie(movie("local-work", "花样年华", 2000, ("zh-CN", "本地简介")));

    let result = configured_fixer().scrape(scanned).resolve().await.unwrap();

    assert_eq!(result.selected_target().media_kind(), MediaKind::Movie);
    assert_eq!(
        result.selected_target().provider().as_str(),
        "fixture.remote"
    );
    let ScrapedMedia::Movie(resolved) = result.media() else {
        panic!("expected movie result");
    };
    assert_eq!(resolved.value.summaries.entries().len(), 2);
    assert!(!resolved.provenance.sources_for("movie.titles").is_empty());
}

#[tokio::test]
async fn exact_scrape_fetches_the_requested_provider_id_and_merges_local_fields() {
    let scanned =
        MetadataDocument::Movie(movie("local-work", "花样年华", 2000, ("zh-CN", "本地简介")));
    let target = ProviderTarget::new(
        MediaKind::Movie,
        ProviderId::new("fixture.remote").unwrap(),
        ExternalId::new("fixture.remote", "movie-remote").unwrap(),
    )
    .unwrap();

    let result = configured_fixer()
        .scrape(scanned)
        .selection(ScrapeSelection::Exact(target.clone()))
        .resolve()
        .await
        .unwrap();

    assert_eq!(result.selected_target(), &target);
    let ScrapedMedia::Movie(resolved) = result.media() else {
        panic!("expected movie result");
    };
    assert_eq!(resolved.value.summaries.entries().len(), 2);
    assert!(
        resolved
            .provenance
            .sources_for("movie.summaries")
            .iter()
            .any(|source| source.provider.as_str() == "fixture.remote")
    );
}
