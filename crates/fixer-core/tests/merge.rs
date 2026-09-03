use fixer_core::{
    ArtworkKind, ArtworkReference, Credit, CreditRole, ExternalId, FieldPath, Genre, MergePolicy,
    MetadataDocument, Movie, MovieDocument, MovieMerger, Person, PersonId, ProviderId, Rating,
    ReleaseDate, ReleaseId, SourceRef, WorkId,
};
use std::time::UNIX_EPOCH;

fn source(provider: &str, id: &str) -> SourceRef {
    SourceRef::new(
        ProviderId::new(provider).unwrap(),
        Some(ExternalId::new(provider, id).unwrap()),
        None,
        UNIX_EPOCH,
    )
}

fn movie_document(
    provider: &str,
    english_title: &str,
    localized_title: (&str, &str),
    summary: &str,
) -> MovieDocument {
    let mut titles = fixer_core::LocalizedValue::new();
    titles.insert("en", english_title.to_owned()).unwrap();
    titles
        .insert(localized_title.0, localized_title.1.to_owned())
        .unwrap();
    let mut movie = Movie::new(WorkId::new(format!("{provider}-movie")).unwrap(), titles);
    movie.summaries.insert("en", summary.to_owned()).unwrap();
    movie.releases.push(fixer_core::MovieRelease::new(
        ReleaseId::new(format!("{provider}-release")).unwrap(),
        ReleaseDate::ymd(2000, 9, 29).unwrap(),
    ));
    MovieDocument::new(movie, source(provider, "843"))
}

#[test]
fn movie_merge_preserves_locales_and_exposes_conflicts() {
    let mut local = movie_document(
        "local",
        "In the Mood for Love",
        ("zh-CN", "花样年华"),
        "Local summary",
    );
    local.value.genres.push(Genre::new("drama").unwrap());
    local
        .value
        .ratings
        .push(Rating::new("local", 9.0, 10.0).unwrap());
    local
        .value
        .ratings
        .push(Rating::new("imdb", 7.0, 10.0).unwrap());
    local.value.artwork.push(
        ArtworkReference::new(ArtworkKind::Poster, "local/poster.jpg")
            .unwrap()
            .with_external_id(ExternalId::new("tmdb", "poster-1").unwrap()),
    );
    local.value.credits.push(Credit::new(
        Person::new(PersonId::new("wong-kar-wai").unwrap(), "Wong Kar-wai").unwrap(),
        CreditRole::Director,
    ));

    let mut tmdb = movie_document(
        "tmdb",
        "In the Mood for Love",
        ("zh-TW", "花樣年華"),
        "TMDB summary",
    );
    tmdb.value
        .ratings
        .push(Rating::new("tmdb", 8.1, 10.0).unwrap());
    tmdb.value
        .ratings
        .push(Rating::new("imdb", 8.0, 10.0).unwrap());
    tmdb.value.artwork.push(
        ArtworkReference::new(ArtworkKind::Poster, "https://image/poster.jpg")
            .unwrap()
            .with_external_id(ExternalId::new("tmdb", "poster-1").unwrap()),
    );
    tmdb.value
        .artwork
        .push(ArtworkReference::new(ArtworkKind::Backdrop, "https://image/backdrop.jpg").unwrap());
    tmdb.value.credits.push(Credit::new(
        Person::new(PersonId::new("wong-kar-wai").unwrap(), "Wong Kar Wai").unwrap(),
        CreditRole::Director,
    ));

    let policy = MergePolicy::new([
        ProviderId::new("tmdb").unwrap(),
        ProviderId::new("local").unwrap(),
    ])
    .with_media_order(
        fixer_core::MediaKind::Movie,
        [
            ProviderId::new("local").unwrap(),
            ProviderId::new("tmdb").unwrap(),
        ],
    )
    .with_field_order(
        FieldPath::new("movie.summaries").unwrap(),
        [
            ProviderId::new("local").unwrap(),
            ProviderId::new("tmdb").unwrap(),
        ],
    )
    .with_field_order(
        FieldPath::new("movie.ratings").unwrap(),
        [
            ProviderId::new("tmdb").unwrap(),
            ProviderId::new("local").unwrap(),
        ],
    );

    let resolved = MovieMerger::new(policy).merge([tmdb, local]).unwrap();
    assert_eq!(resolved.value.titles.entries().len(), 3);
    assert_eq!(resolved.value.summaries.entries().len(), 1);
    assert_eq!(
        resolved.value.summaries.entries()[0].value(),
        "Local summary"
    );
    assert_eq!(resolved.value.credits.len(), 1);
    assert_eq!(resolved.value.ratings.len(), 3);
    let imdb_rating = resolved
        .value
        .ratings
        .iter()
        .find(|rating| rating.system == "imdb")
        .unwrap()
        .value;
    assert!((imdb_rating - 8.0).abs() < f32::EPSILON);
    assert_eq!(resolved.value.artwork.len(), 2);
    assert!(!resolved.conflicts.is_empty());
    assert!(!resolved.provenance.sources_for("movie.titles").is_empty());
    assert!(resolved.completeness > 0.0);
}

#[test]
fn normalized_identity_deduplicates_people_without_stable_ids() {
    let mut first = movie_document("one", "Movie", ("fr", "Film"), "Summary");
    first.value.credits.push(Credit::new(
        Person::new(PersonId::new("provider-one-1").unwrap(), "Maggie Cheung").unwrap(),
        CreditRole::Actor,
    ));
    let mut second = movie_document("two", "Movie", ("de", "Film"), "Summary");
    second.value.credits.push(Credit::new(
        Person::new(
            PersonId::new("provider-two-9").unwrap(),
            "  maggie   CHEUNG ",
        )
        .unwrap(),
        CreditRole::Actor,
    ));

    let resolved = MovieMerger::new(MergePolicy::new([
        ProviderId::new("one").unwrap(),
        ProviderId::new("two").unwrap(),
    ]))
    .merge([first, second])
    .unwrap();

    assert_eq!(resolved.value.credits.len(), 1);
}

#[test]
fn unsupported_merge_document_combinations_are_explicit() {
    let document = MetadataDocument::Movie(
        movie_document("local", "Movie", ("zh-CN", "电影"), "Summary").value,
    );
    let error = MovieMerger::new(MergePolicy::new([ProviderId::new("local").unwrap()]))
        .merge_documents([document])
        .unwrap_err();
    assert!(matches!(error, fixer_core::MergeError::MissingSource));
}
