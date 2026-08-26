use fixer_core::{OutputOperation, OutputPlan};
use fixer_server::FsPolicy;

#[test]
fn canonical_roots_allow_existing_reads_and_future_writes_beneath_them() {
    let root = tempfile::tempdir().unwrap();
    let library = root.path().join("library");
    std::fs::create_dir(&library).unwrap();
    let media = library.join("movie.mkv");
    std::fs::write(&media, b"media").unwrap();
    let policy = FsPolicy::new([&library]).unwrap();

    assert_eq!(
        policy.validate_read(&media).unwrap(),
        media.canonicalize().unwrap()
    );
    assert_eq!(
        policy
            .validate_write(library.join("metadata/movie.json"))
            .unwrap(),
        library.join("metadata/movie.json")
    );
    assert!(
        policy
            .validate_read(root.path().join("outside.mkv"))
            .is_err()
    );
    assert!(
        policy
            .validate_write(root.path().join("outside.json"))
            .is_err()
    );
}

#[cfg(unix)]
#[test]
fn symlinks_cannot_escape_for_reads_or_future_writes() {
    use std::os::unix::fs::symlink;

    let root = tempfile::tempdir().unwrap();
    let library = root.path().join("library");
    let outside = root.path().join("outside");
    std::fs::create_dir(&library).unwrap();
    std::fs::create_dir(&outside).unwrap();
    std::fs::write(outside.join("secret.mkv"), b"secret").unwrap();
    symlink(&outside, library.join("escape")).unwrap();
    let policy = FsPolicy::new([&library]).unwrap();

    assert!(
        policy
            .validate_read(library.join("escape/secret.mkv"))
            .is_err()
    );
    assert!(
        policy
            .validate_write(library.join("escape/new.json"))
            .is_err()
    );
}

#[test]
fn every_output_plan_source_and_target_must_stay_in_an_allowed_root() {
    let root = tempfile::tempdir().unwrap();
    let library = root.path().join("library");
    let outside = root.path().join("outside");
    std::fs::create_dir(&library).unwrap();
    std::fs::create_dir(&outside).unwrap();
    let source = library.join("source.mkv");
    std::fs::write(&source, b"media").unwrap();
    let policy = FsPolicy::new([&library]).unwrap();

    let mut allowed = OutputPlan::new(&library);
    allowed.push(OutputOperation::Copy {
        source: source.clone(),
        target: library.join("copy.mkv"),
    });
    policy.validate_plan(&allowed).unwrap();

    let mut escaped_target = OutputPlan::new(&library);
    escaped_target.push(OutputOperation::Copy {
        source: source.clone(),
        target: outside.join("copy.mkv"),
    });
    assert!(policy.validate_plan(&escaped_target).is_err());

    let mut escaped_source = OutputPlan::new(&library);
    escaped_source.push(OutputOperation::Copy {
        source: outside.join("missing.mkv"),
        target: library.join("copy.mkv"),
    });
    assert!(policy.validate_plan(&escaped_source).is_err());
}
