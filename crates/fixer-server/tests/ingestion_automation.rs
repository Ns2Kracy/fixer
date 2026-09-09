use std::{num::NonZeroUsize, path::PathBuf, time::Duration};

use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode, header},
};
use fixer_core::{ExternalId, LocalizedValue, MetadataDocument, Movie, ProviderId, WorkId};
use fixer_sdk::{Fixer, FixtureDocument, FixtureProvider};
use fixer_server::{
    FsPolicy, JobRuntime, SdkJobFlow, SqliteJobStore,
    ingestion::model::RulePlacement,
    job_app,
    jobs::model::{JobInputDto, JobMediaKind, JobOrganizationDto},
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tempfile::TempDir;
use tokio::time::{sleep, timeout};
use tower::ServiceExt;

struct AutomationApp {
    _root: TempDir,
    source: PathBuf,
    destination: PathBuf,
    store: SqliteJobStore,
    runtime: JobRuntime,
    router: Router,
}

impl AutomationApp {
    async fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("incoming");
        let destination = root.path().join("library");
        std::fs::create_dir_all(&source).unwrap();
        std::fs::create_dir_all(&destination).unwrap();
        let store = SqliteJobStore::open(root.path().join("automation.sqlite3"))
            .await
            .unwrap();
        let policy = FsPolicy::new([root.path()]).unwrap();
        let runtime =
            JobRuntime::new(store.clone(), NonZeroUsize::new(32).unwrap()).with_fs_policy(policy);
        let router = job_app(runtime.clone());
        Self {
            _root: root,
            source,
            destination,
            store,
            runtime,
            router,
        }
    }

    fn start_workers(&self) -> fixer_server::WorkerPool {
        self.runtime.start_workers(
            NonZeroUsize::new(1).unwrap(),
            SdkJobFlow::new(fixture_fixer()),
        )
    }

    fn write_source(&self) -> PathBuf {
        let media = self.source.join("Fixture Movie.mkv");
        std::fs::write(&media, b"source fixture").unwrap();
        media
    }

    async fn enqueue_trusted_snapshot(&self) {
        let input = JobInputDto::new(
            JobMediaKind::Movie,
            self.write_source().to_string_lossy(),
            true,
        )
        .with_organization(JobOrganizationDto {
            destination_path: self.destination.to_string_lossy().into_owned(),
            placement: RulePlacement::Copy,
            path_template: Some("Auto/{{ title | sanitize }}".to_owned()),
            origin_rule_id: Some(999),
            auto_execute: true,
        });
        self.store.create_job(input).await.unwrap();
    }

    async fn create_manual_attempting_automatic_execution(&self) {
        let response = send(
            &self.router,
            Request::post("/api/v1/jobs")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "media_kind": "movie",
                        "input_path": self.write_source(),
                        "apply": true,
                        "organization": {
                            "destination_path": self.destination,
                            "placement": "copy",
                            "path_template": "Auto/{{ title | sanitize }}",
                            "origin_rule_id": 999,
                            "auto_execute": true
                        }
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::ACCEPTED);
        assert!(
            !response_json(response).await["job"]["input"]["organization"]["auto_execute"]
                .as_bool()
                .unwrap()
        );
    }

    async fn wait_for_state(&self, expected: &str) -> Value {
        timeout(Duration::from_secs(3), async {
            loop {
                let response = send(
                    &self.router,
                    Request::get("/api/v1/jobs/1").body(Body::empty()).unwrap(),
                )
                .await;
                let job = response_json(response).await;
                if job["job"]["state"] == expected {
                    return job;
                }
                sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap_or_else(|_| panic!("job did not reach {expected}"))
    }
}

fn fixture_fixer() -> Fixer {
    let mut titles = LocalizedValue::new();
    titles.insert("en", "Fixture Movie".to_owned()).unwrap();
    let movie = Movie::new(WorkId::new("fixture-movie").unwrap(), titles);
    let provider = FixtureProvider::new(
        ProviderId::new("fixture.automation").unwrap(),
        [FixtureDocument::new(
            ExternalId::new("fixture.automation", "fixture-movie").unwrap(),
            MetadataDocument::Movie(movie),
        )],
    )
    .unwrap();
    Fixer::builder()
        .provider(provider)
        .offline()
        .build()
        .unwrap()
}

async fn send(router: &Router, request: Request<Body>) -> axum::response::Response {
    router.clone().oneshot(request).await.unwrap()
}

async fn response_json(response: axum::response::Response) -> Value {
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

#[tokio::test]
async fn orphaned_rule_snapshot_auto_reviews_and_executes_a_safe_unique_match() {
    let app = AutomationApp::new().await;
    app.enqueue_trusted_snapshot().await;
    let workers = app.start_workers();

    let completed = app.wait_for_state("completed").await;
    assert_eq!(completed["job"]["review_decision"]["candidate_index"], 0);
    assert!(completed["job"]["plan"]["fingerprint"].is_string());
    assert_eq!(completed["job"]["execution"]["failed_operations"], 0);
    assert_eq!(
        std::fs::read(app.destination.join("Auto/Fixture Movie/Fixture Movie.mkv")).unwrap(),
        b"source fixture"
    );

    workers.shutdown().await;
}

#[tokio::test]
async fn manual_job_with_the_same_match_still_waits_for_confirmation() {
    let app = AutomationApp::new().await;
    app.create_manual_attempting_automatic_execution().await;
    let workers = app.start_workers();

    let awaiting = app.wait_for_state("awaiting_confirmation").await;
    assert_eq!(awaiting["job"]["review"]["automation_reason"], "manual_job");
    assert!(!app.destination.join("Auto/Fixture Movie").exists());

    workers.shutdown().await;
}

#[tokio::test]
async fn destination_collision_fails_without_entering_the_writer() {
    let app = AutomationApp::new().await;
    let collision = app.destination.join("Auto/Fixture Movie/Fixture Movie.mkv");
    std::fs::create_dir_all(collision.parent().unwrap()).unwrap();
    std::fs::write(&collision, b"keep me").unwrap();
    app.enqueue_trusted_snapshot().await;
    let workers = app.start_workers();

    let failed = app.wait_for_state("failed").await;
    assert_eq!(
        failed["job"]["execution"]["failure"]["code"],
        "automatic_scrape_blocked"
    );
    assert_eq!(
        failed["job"]["review"]["automation_reason"],
        "destination_collision"
    );
    assert_eq!(std::fs::read(collision).unwrap(), b"keep me");

    workers.shutdown().await;
}
