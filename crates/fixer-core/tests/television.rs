use fixer_core::{
    Episode, EpisodeSequence, ExternalId, MatchQuery, Matcher, MediaKind, MergePolicy,
    MetadataDocument, OrderingScheme, ProviderId, SearchRequest, Season, Series, SeriesDocument,
    SeriesMerger, SourceRef, TelevisionCandidate, WorkId,
};
use std::time::UNIX_EPOCH;

fn titles(language: &str, value: &str) -> fixer_core::LocalizedValue<String> {
    let mut titles = fixer_core::LocalizedValue::new();
    titles.insert(language, value.to_owned()).unwrap();
    titles
}

fn source(provider: &str, id: &str) -> SourceRef {
    SourceRef::new(
        ProviderId::new(provider).unwrap(),
        Some(ExternalId::new(provider, id).unwrap()),
        None,
        UNIX_EPOCH,
    )
}

fn series_document(provider: &str, summary: &str, episode_title: &str) -> SeriesDocument {
    let mut episode = Episode::new(
        WorkId::new(format!("{provider}-episode")).unwrap(),
        titles("en", episode_title),
        EpisodeSequence::aired(1, 2).unwrap(),
    );
    episode
        .summaries
        .insert("en", format!("{provider} episode summary"))
        .unwrap();
    let season = Season::new(
        WorkId::new(format!("{provider}-season")).unwrap(),
        1,
        vec![episode],
    )
    .unwrap();
    let mut series = Series::new(
        WorkId::new(format!("{provider}-series")).unwrap(),
        titles("en", "Example Show"),
        OrderingScheme::Aired,
        vec![season],
    );
    series.summaries.insert("en", summary.to_owned()).unwrap();
    SeriesDocument::new(series, source(provider, "1399"))
}

#[test]
fn television_requests_and_matching_are_typed() {
    let request = SearchRequest::television("Example Show", Some(2011)).unwrap();
    assert_eq!(request.media_kind(), MediaKind::Television);

    let candidate = TelevisionCandidate::new(
        ProviderId::new("tmdb").unwrap(),
        ExternalId::new("tmdb", "1399").unwrap(),
        "Example Show",
        Some(2011),
    )
    .unwrap();
    let selection = Matcher
        .select(
            &MatchQuery::television("Example Show")
                .unwrap()
                .with_year(2011)
                .with_external_id(ExternalId::new("tmdb", "1399").unwrap()),
            vec![fixer_core::Candidate::Television(candidate)],
        )
        .unwrap();
    assert!(selection.selected().is_some());
}

#[test]
fn series_merge_preserves_hierarchy_and_field_provenance() {
    let local = series_document("local", "Local summary", "Local episode title");
    let tmdb = series_document("tmdb", "TMDB summary", "TMDB episode title");
    let policy = MergePolicy::new([
        ProviderId::new("local").unwrap(),
        ProviderId::new("tmdb").unwrap(),
    ]);

    let resolved = SeriesMerger::new(policy).merge([tmdb, local]).unwrap();

    assert_eq!(resolved.value.ordering, OrderingScheme::Aired);
    assert_eq!(resolved.value.seasons.len(), 1);
    assert_eq!(resolved.value.seasons[0].number, 1);
    assert_eq!(resolved.value.seasons[0].episodes.len(), 1);
    assert_eq!(
        resolved.value.seasons[0].episodes[0].sequence,
        EpisodeSequence::aired(1, 2).unwrap()
    );
    assert_eq!(
        resolved.value.summaries.entries()[0].value(),
        "Local summary"
    );
    assert_eq!(
        resolved.value.seasons[0].episodes[0].titles.entries()[0].value(),
        "Local episode title"
    );
    assert!(
        !resolved
            .provenance
            .sources_for("series.seasons.1.episodes.2.titles")
            .is_empty()
    );
}

#[test]
fn series_merger_rejects_mixed_ordering_schemes() {
    let local = series_document("local", "Summary", "Episode");
    let mut tmdb = series_document("tmdb", "Summary", "Episode");
    tmdb.value.ordering = OrderingScheme::Dvd;
    tmdb.value.seasons[0].episodes[0].sequence.scheme = OrderingScheme::Dvd;
    let error = SeriesMerger::new(MergePolicy::new([
        ProviderId::new("local").unwrap(),
        ProviderId::new("tmdb").unwrap(),
    ]))
    .merge([local, tmdb])
    .unwrap_err();
    assert!(matches!(
        error,
        fixer_core::MergeError::OrderingMismatch { .. }
    ));
}

#[test]
fn series_merger_rejects_bare_documents_without_source_metadata() {
    let document =
        MetadataDocument::Television(series_document("local", "Summary", "Episode").value);
    let error = SeriesMerger::new(MergePolicy::new([ProviderId::new("local").unwrap()]))
        .merge_documents([document])
        .unwrap_err();
    assert!(matches!(error, fixer_core::MergeError::MissingSource));
}
