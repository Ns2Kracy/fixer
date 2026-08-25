use fixer_provider_local::{EvidenceKind, identify_path};
use std::path::Path;

#[test]
fn identifies_common_movie_filename() {
    let hint = identify_path(Path::new("In.the.Mood.for.Love.2000.1080p.BluRay.mkv")).unwrap();
    assert_eq!(hint.title, "In the Mood for Love");
    assert_eq!(hint.year, Some(2000));
    assert!(
        hint.evidence
            .iter()
            .any(|item| item.kind == EvidenceKind::Filename)
    );
}

#[test]
fn nested_movie_directory_contributes_evidence() {
    let hint = identify_path(Path::new("Movies/In the Mood for Love (2000)/movie.mkv")).unwrap();
    assert_eq!(hint.title, "In the Mood for Love");
    assert_eq!(hint.year, Some(2000));
    assert!(
        hint.evidence
            .iter()
            .any(|item| item.kind == EvidenceKind::Directory)
    );
}

#[test]
fn malformed_or_implausible_year_is_not_promoted() {
    let hint = identify_path(Path::new("Movie (99999).mkv")).unwrap();
    assert_eq!(hint.title, "Movie (99999)");
    assert_eq!(hint.year, None);
}
