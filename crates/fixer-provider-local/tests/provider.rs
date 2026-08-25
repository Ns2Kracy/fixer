use fixer_core::{
    FetchRequest, HttpClient, HttpError, HttpRequest, HttpResponse, MediaKind, Provider,
    SearchRequest,
};
use fixer_provider_local::{LocalProvider, parse_json, parse_nfo, scan};
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
fn parses_supported_nfo_subset() {
    let movie = parse_nfo(include_str!("fixtures/movie.nfo")).unwrap();
    assert_eq!(movie.release_year(), Some(2000));
    assert_eq!(
        movie.summaries.entries()[0].value(),
        "A Cantonese-language romantic drama."
    );
}

#[test]
fn parses_sanitized_local_json() {
    let movie = parse_json(include_str!("fixtures/movie.json")).unwrap();
    assert_eq!(movie.release_year(), Some(2000));
    assert_eq!(movie.titles.entries().len(), 2);
}

#[test]
fn malformed_files_become_path_warnings() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("broken.json"), "{not-json").unwrap();
    let result = scan(root.path()).unwrap();
    assert!(
        result
            .warnings
            .iter()
            .any(|warning| warning.path.ends_with("broken.json"))
    );
}

#[test]
fn provider_is_local_and_searches_without_http() {
    let provider =
        LocalProvider::from_documents([parse_json(include_str!("fixtures/movie.json")).unwrap()])
            .unwrap();
    assert!(!provider.descriptor().requires_network());
    let candidates = futures_lite::future::block_on(provider.search(
        SearchRequest::movie("花样年华", Some(2000)).unwrap(),
        &Offline,
    ))
    .unwrap();
    assert_eq!(candidates.len(), 1);
    let fetched = futures_lite::future::block_on(provider.fetch(
        FetchRequest::new(MediaKind::Movie, candidates[0].external_id().clone()),
        &Offline,
    ))
    .unwrap();
    assert_eq!(fetched.media_kind(), MediaKind::Movie);
}

#[cfg(unix)]
#[test]
fn recursive_scan_does_not_follow_symlinks_outside_root() {
    use std::os::unix::fs::symlink;
    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::fs::write(
        outside.path().join("movie.json"),
        include_str!("fixtures/movie.json"),
    )
    .unwrap();
    symlink(outside.path(), root.path().join("escape")).unwrap();

    let result = scan(root.path()).unwrap();
    assert!(result.documents.is_empty());
    assert!(
        result
            .warnings
            .iter()
            .any(|warning| warning.path == root.path().join("escape"))
    );
}

#[test]
fn scan_rejects_a_non_directory_root() {
    let root = tempfile::tempdir().unwrap();
    let file = root.path().join("file");
    std::fs::write(&file, "x").unwrap();
    assert!(scan(Path::new(&file)).is_err());
}
