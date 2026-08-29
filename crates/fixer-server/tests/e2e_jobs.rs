use std::num::NonZeroUsize;

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use fixer_server::{
    JobRuntime, SqliteJobStore, job_app,
    jobs::model::{JobInputDto, JobMediaKind, JobState, ProgressSummary},
    store::JobUpdate,
};
use http_body_util::BodyExt;
use serde_json::Value;
use tower::ServiceExt;

async fn response_json(response: axum::response::Response) -> Value {
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

#[tokio::test]
async fn reopening_the_server_store_interrupts_active_jobs() {
    let directory = tempfile::tempdir().unwrap();
    let database = directory.path().join("jobs.sqlite");
    let job_id = {
        let store = SqliteJobStore::open(&database).await.unwrap();
        let job = store
            .create_job(JobInputDto::new(
                JobMediaKind::Movie,
                directory
                    .path()
                    .join("Fixture Movie.mkv")
                    .display()
                    .to_string(),
                false,
            ))
            .await
            .unwrap();
        let active = store
            .transition(
                job.id(),
                JobState::Queued,
                JobState::Scanning,
                JobUpdate::default().with_progress(ProgressSummary::new("scanning", 0, None)),
            )
            .await
            .unwrap();
        assert_eq!(active.state(), JobState::Scanning);
        active.id()
    };

    let restarted = SqliteJobStore::open(&database).await.unwrap();
    let recovered = restarted.get_job(job_id).await.unwrap();
    assert_eq!(recovered.state(), JobState::Interrupted);

    let router = job_app(JobRuntime::new(restarted, NonZeroUsize::new(8).unwrap()));
    let response = router
        .oneshot(
            Request::get(format!("/api/v1/jobs/{}", job_id.get()))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = response_json(response).await;
    assert_eq!(body["job"]["state"], "interrupted");
    assert_eq!(body["job"]["id"], job_id.get());
}
