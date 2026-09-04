use std::{
    fs,
    io::{Cursor, Write},
    path::{Path, PathBuf},
};

use fixer_server::{
    ingestion::{
        discovery::{DiscoveryOutcome, discover, reconcile_event},
        model::MediaKindMode,
    },
    jobs::model::JobMediaKind,
};
use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

fn fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/library")
}

fn copy_movie_nfo(target: &Path, title: &str) {
    fs::create_dir_all(target).unwrap();
    let fixture =
        fs::read_to_string(fixture_root().join("movie/In the Mood for Love (2000)/movie.nfo"))
            .unwrap();
    fs::write(
        target.join("movie.nfo"),
        fixture.replace("In the Mood for Love", title),
    )
    .unwrap();
}

fn write_epub(path: &Path, title: &str, isbn: &str) {
    let opf = format!(
        r#"<package xmlns:dc="http://purl.org/dc/elements/1.1/"><metadata>
<dc:identifier>urn:isbn:{isbn}</dc:identifier><dc:title>{title}</dc:title>
<dc:creator>Fixture Author</dc:creator><dc:publisher>Fixture Press</dc:publisher>
</metadata></package>"#
    );
    let mut archive = ZipWriter::new(Cursor::new(Vec::new()));
    let stored = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    let deflated = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    archive.start_file("mimetype", stored).unwrap();
    archive.write_all(b"application/epub+zip").unwrap();
    archive
        .start_file("META-INF/container.xml", deflated)
        .unwrap();
    archive
        .write_all(
            br#"<container><rootfiles><rootfile full-path="OPS/content.opf"/></rootfiles></container>"#,
        )
        .unwrap();
    archive.start_file("OPS/content.opf", deflated).unwrap();
    archive.write_all(opf.as_bytes()).unwrap();
    fs::write(path, archive.finish().unwrap().into_inner()).unwrap();
}

fn ready_count(items: &[fixer_server::ingestion::discovery::DiscoveredItem]) -> usize {
    items
        .iter()
        .filter(|item| matches!(item.outcome(), DiscoveryOutcome::Ready(_)))
        .count()
}

#[test]
fn fixed_movie_directory_emits_one_job_per_movie() {
    let source = tempfile::tempdir().unwrap();
    let first = source.path().join("Arrival (2016)");
    let second = source.path().join("Moon (2009)");
    copy_movie_nfo(&first, "Arrival");
    copy_movie_nfo(&second, "Moon");

    let items = discover(source.path(), MediaKindMode::Fixed(JobMediaKind::Movie)).unwrap();
    let roots = items
        .iter()
        .filter(|item| item.media_kind() == Some(JobMediaKind::Movie))
        .map(|item| item.source_root().to_path_buf())
        .collect::<Vec<_>>();

    assert_eq!(
        roots,
        vec![
            first.canonicalize().unwrap(),
            second.canonicalize().unwrap()
        ]
    );
}

#[test]
fn fixed_modes_emit_nested_series_releases_and_books() {
    let television = tempfile::tempdir().unwrap();
    for title in ["First Show", "Second Show"] {
        let season = television.path().join(title).join("Season 01");
        fs::create_dir_all(&season).unwrap();
        fs::write(season.join(format!("{title}.S01E01.mkv")), b"").unwrap();
    }
    let television_items = discover(
        television.path(),
        MediaKindMode::Fixed(JobMediaKind::Television),
    )
    .unwrap();
    assert_eq!(ready_count(&television_items), 2);

    let anime_items = discover(
        fixture_root().join("anime"),
        MediaKindMode::Fixed(JobMediaKind::Anime),
    )
    .unwrap();
    assert_eq!(ready_count(&anime_items), 2);

    let music_items = discover(
        fixture_root().join("music"),
        MediaKindMode::Fixed(JobMediaKind::Music),
    )
    .unwrap();
    assert_eq!(ready_count(&music_items), 1);

    let books = tempfile::tempdir().unwrap();
    for (directory, title, isbn) in [
        ("Book A", "First Book", "9780441478125"),
        ("Book B", "Second Book", "9781473225947"),
    ] {
        let root = books.path().join(directory);
        fs::create_dir_all(&root).unwrap();
        write_epub(&root.join("book.epub"), title, isbn);
    }
    let book_items = discover(books.path(), MediaKindMode::Fixed(JobMediaKind::Book)).unwrap();
    assert_eq!(ready_count(&book_items), 2);
}

#[test]
fn fixed_mode_does_not_emit_other_media_types() {
    let items = discover(fixture_root(), MediaKindMode::Fixed(JobMediaKind::Movie)).unwrap();

    assert_eq!(ready_count(&items), 1);
    assert!(
        items
            .iter()
            .filter_map(|item| item.media_kind())
            .all(|kind| kind == JobMediaKind::Movie)
    );
}

#[test]
fn auto_mode_emits_unique_claims_and_bounds_ambiguous_roots() {
    let unique = discover(fixture_root().join("movie"), MediaKindMode::Auto).unwrap();
    assert_eq!(ready_count(&unique), 1);
    assert_eq!(unique[0].media_kind(), Some(JobMediaKind::Movie));

    let source = tempfile::tempdir().unwrap();
    let shared = source.path().join("Shared");
    copy_movie_nfo(&shared, "Shared");
    fs::write(shared.join("Shared.S01E01.mkv"), b"").unwrap();
    let ambiguous = discover(source.path(), MediaKindMode::Auto).unwrap();
    let outcome = ambiguous
        .iter()
        .find(|item| item.source_root() == shared.canonicalize().unwrap())
        .unwrap()
        .outcome();
    assert_eq!(
        outcome,
        &DiscoveryOutcome::NeedsReview {
            media_kinds: vec![JobMediaKind::Movie, JobMediaKind::Television]
        }
    );
}

#[test]
fn unsupported_entries_are_ignored_and_duplicate_roots_are_collapsed() {
    let source = tempfile::tempdir().unwrap();
    fs::write(source.path().join("notes.txt"), "unsupported").unwrap();
    let ignored = discover(source.path(), MediaKindMode::Auto).unwrap();
    assert_eq!(ignored.len(), 1);
    assert!(matches!(ignored[0].outcome(), DiscoveryOutcome::Ignored));

    let duplicate = source.path().join("Duplicate");
    copy_movie_nfo(&duplicate, "Duplicate");
    fs::copy(duplicate.join("movie.nfo"), duplicate.join("other.nfo")).unwrap();
    let items = discover(&duplicate, MediaKindMode::Fixed(JobMediaKind::Movie)).unwrap();
    assert_eq!(ready_count(&items), 1);
}

#[test]
fn event_reconciliation_selects_the_nearest_discovered_root() {
    let source = tempfile::tempdir().unwrap();
    let collection = source.path().join("Collection");
    copy_movie_nfo(&collection, "Collection");
    let anime = collection.join("Nested Anime");
    fs::create_dir_all(&anime).unwrap();
    fs::copy(
        fixture_root().join("anime/Fixture Journey A/anime.nfo"),
        anime.join("anime.nfo"),
    )
    .unwrap();

    let items = discover(source.path(), MediaKindMode::Auto).unwrap();
    let matched = reconcile_event(&items, anime.join("Season 01/new-file.mkv"))
        .unwrap()
        .unwrap();

    assert_eq!(matched.source_root(), anime.canonicalize().unwrap());
    assert_eq!(matched.media_kind(), Some(JobMediaKind::Anime));
}
