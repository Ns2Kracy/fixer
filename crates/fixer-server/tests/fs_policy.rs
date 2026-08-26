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

#[tokio::test]
async fn configured_job_runtime_rejects_outside_inputs_before_persistence() {
    use std::num::NonZeroUsize;

    use axum::{
        body::Body,
        http::{Request, StatusCode, header},
    };
    use fixer_server::{JobRuntime, SqliteJobStore, job_app};
    use serde_json::json;
    use tower::ServiceExt;

    let root = tempfile::tempdir().unwrap();
    let library = root.path().join("library");
    std::fs::create_dir(&library).unwrap();
    let inside = library.join("inside.mkv");
    let outside = root.path().join("outside.mkv");
    std::fs::write(&inside, b"inside").unwrap();
    std::fs::write(&outside, b"outside").unwrap();
    let store = SqliteJobStore::open(root.path().join("jobs.sqlite3"))
        .await
        .unwrap();
    let runtime = JobRuntime::new(store.clone(), NonZeroUsize::new(8).unwrap())
        .with_fs_policy(FsPolicy::new([&library]).unwrap());
    let router = job_app(runtime);

    let create = |path: &std::path::Path| {
        Request::post("/api/v1/jobs")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                json!({"media_kind": "movie", "input_path": path, "apply": false}).to_string(),
            ))
            .unwrap()
    };
    let rejected = router.clone().oneshot(create(&outside)).await.unwrap();
    assert_eq!(rejected.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let accepted = router.oneshot(create(&inside)).await.unwrap();
    assert_eq!(accepted.status(), StatusCode::ACCEPTED);
    let body = http_body_util::BodyExt::collect(accepted.into_body())
        .await
        .unwrap()
        .to_bytes();
    let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(value["job"]["id"], 1);
}
