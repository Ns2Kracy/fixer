use fixer_core::{
    FetchRequest, HttpClient, HttpError, HttpRequest, HttpResponse, MediaKind, OrderingScheme,
    Provider, SearchRequest,
};
use fixer_provider_local::{
    LocalError, LocalProvider, identify_episode_path, parse_matroska_tags, scan_television,
};
use std::path::Path;

struct Offline;
impl HttpClient for Offline {
    fn execute<'a>(
        &'a self,
        _: HttpRequest,
    ) -> fixer_core::BoxFuture<'a, Result<HttpResponse, HttpError>> {
        Box::pin(async { Err(HttpError::Offline) })
    }
}

#[test]
fn recognizes_sxe_season_folders_specials_and_external_ids() {
    let named = identify_episode_path(Path::new(
        "Example Show/Season 01/Example.Show.S01E02.{tmdb-1399}.mkv",
    ))
    .unwrap();
    assert_eq!(named.series_title, "Example Show");
    assert_eq!(named.sequence.season, Some(1));
    assert_eq!(named.sequence.episode, 2);
    assert_eq!(named.external_ids[0].namespace, "tmdb");
    assert_eq!(named.external_ids[0].value, "1399");

    let folder = identify_episode_path(Path::new(
        "Example Show/Season 02/03 - The Third Episode.mkv",
    ))
    .unwrap();
    assert_eq!(folder.sequence.season, Some(2));
    assert_eq!(folder.sequence.episode, 3);
    assert_eq!(folder.episode_title.as_deref(), Some("The Third Episode"));

    let special =
        identify_episode_path(Path::new("Example Show/Specials/Example.Show.S00E01.mkv")).unwrap();
    assert_eq!(special.sequence.season, Some(0));
    assert_eq!(special.sequence.episode, 1);
}

#[test]
fn parses_matroska_tags_without_flattening_sequence() {
    let tags = parse_matroska_tags(
        r#"<Tags><Tag>
            <Simple><Name>TVSHOW</Name><String>Example Show</String></Simple>
            <Simple><Name>TITLE</Name><String>The Tagged Episode</String></Simple>
            <Simple><Name>SEASON</Name><String>1</String></Simple>
            <Simple><Name>EPISODE</Name><String>2</String></Simple>
            <Simple><Name>ORDERING</Name><String>dvd</String></Simple>
            <Simple><Name>TMDBID</Name><String>1399</String></Simple>
        </Tag></Tags>"#,
    )
    .unwrap();
    assert_eq!(tags.series_title.as_deref(), Some("Example Show"));
    assert_eq!(tags.episode_title.as_deref(), Some("The Tagged Episode"));
    assert_eq!(tags.ordering, Some(OrderingScheme::Dvd));
    assert_eq!(tags.season, Some(1));
    assert_eq!(tags.episode, Some(2));
    assert_eq!(tags.external_ids[0].namespace, "tmdb");
}

#[test]
fn scans_episode_files_into_series_season_episode_hierarchy() {
    let root = tempfile::tempdir().unwrap();
    let season = root.path().join("Example Show").join("Season 01");
    std::fs::create_dir_all(&season).unwrap();
    std::fs::write(season.join("Example.Show.S01E02.mkv"), []).unwrap();
    std::fs::write(
        season.join("Example.Show.S01E02.tags.xml"),
        r#"<Tags><Tag>
            <Simple><Name>TVSHOW</Name><String>Example Show</String></Simple>
            <Simple><Name>TITLE</Name><String>The Tagged Episode</String></Simple>
            <Simple><Name>SEASON</Name><String>1</String></Simple>
            <Simple><Name>EPISODE</Name><String>2</String></Simple>
        </Tag></Tags>"#,
    )
    .unwrap();

    let result = scan_television(root.path()).unwrap();
    assert!(result.warnings.is_empty());
    assert_eq!(result.documents.len(), 1);
    let series = &result.documents[0];
    assert_eq!(series.seasons.len(), 1);
    assert_eq!(series.seasons[0].number, 1);
    assert_eq!(series.seasons[0].episodes.len(), 1);
    assert_eq!(
        series.seasons[0].episodes[0].titles.entries()[0].value(),
        "The Tagged Episode"
    );
}

#[test]
fn scan_rejects_mixed_ordering_schemes_within_one_series() {
    let root = tempfile::tempdir().unwrap();
    let season = root.path().join("Example Show").join("Season 01");
    std::fs::create_dir_all(&season).unwrap();
    std::fs::write(season.join("Example.Show.S01E01.mkv"), []).unwrap();
    std::fs::write(season.join("Example.Show.S01E02.mkv"), []).unwrap();
    std::fs::write(
        season.join("Example.Show.S01E02.tags.xml"),
        r#"<Tags><Tag>
            <Simple><Name>ORDERING</Name><String>dvd</String></Simple>
        </Tag></Tags>"#,
    )
    .unwrap();

    let error = scan_television(root.path()).unwrap_err();
    assert!(
        matches!(error, LocalError::InvalidMetadata(message) if message.contains("mixed television ordering"))
    );
}

#[test]
fn local_provider_searches_and_fetches_television_without_http() {
    let root = tempfile::tempdir().unwrap();
    let season = root.path().join("Example Show").join("Season 01");
    std::fs::create_dir_all(&season).unwrap();
    std::fs::write(season.join("Example.Show.S01E02.{tmdb-1399}.mkv"), []).unwrap();

    let (provider, warnings) = LocalProvider::from_scan(root.path()).unwrap();
    assert!(warnings.is_empty());
    assert!(provider.descriptor().supports(MediaKind::Television));
    let candidates = futures_lite::future::block_on(provider.search(
        SearchRequest::television("Example Show", None).unwrap(),
        &Offline,
    ))
    .unwrap();
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].external_id().namespace, "tmdb");

    let fetched = futures_lite::future::block_on(provider.fetch(
        FetchRequest::new(MediaKind::Television, candidates[0].external_id().clone()),
        &Offline,
    ))
    .unwrap();
    assert_eq!(fetched.media_kind(), MediaKind::Television);
}
