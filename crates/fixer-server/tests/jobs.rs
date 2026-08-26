use std::{
    num::NonZeroUsize,
    sync::atomic::{AtomicBool, Ordering},
};

use axum::{
    Router,
    body::Body,
    http::{Method, Request, StatusCode, header},
};
use fixer_core::{
    BoxFuture, Candidate, ExternalId, FetchRequest, HttpClient, LocalizedValue, MetadataDocument,
    Movie, Provider, ProviderDescriptor, ProviderError, ProviderId, SearchRequest, WorkId,
};
use fixer_sdk::{Fixer, FixtureDocument, FixtureProvider};
use fixer_server::{JobRuntime, SdkJobFlow, SqliteJobStore, job_app};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tempfile::TempDir;
use tokio::time::{Duration, timeout};
use tower::ServiceExt;

struct TestApp {
    _directory: TempDir,
    store: SqliteJobStore,
    router: Router,
}

impl TestApp {
    async fn new(event_capacity: usize) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let store = SqliteJobStore::open(directory.path().join("jobs.sqlite"))
            .await
            .unwrap();
        let runtime = JobRuntime::new(store.clone(), capacity(event_capacity));
        Self {
            _directory: directory,
            store,
            router: job_app(runtime),
        }
    }

    fn restarted_router(&self, event_capacity: usize) -> Router {
        job_app(JobRuntime::new(
            self.store.clone(),
            capacity(event_capacity),
        ))
    }

    async fn create_movie_job(&self) -> axum::response::Response {
        self.request(
            Request::post("/api/v1/jobs")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "media_kind": "movie",
                        "input_path": "/media/In the Mood for Love (2000).mkv",
                        "apply": false
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
    }

    async fn request(&self, request: Request<Body>) -> axum::response::Response {
        send(&self.router, request).await
    }
}

fn capacity(value: usize) -> NonZeroUsize {
    NonZeroUsize::new(value).unwrap()
}

async fn send(router: &Router, request: Request<Body>) -> axum::response::Response {
    router.clone().oneshot(request).await.unwrap()
}

async fn response_json(response: axum::response::Response) -> Value {
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

async fn first_sse_frame(response: axum::response::Response) -> String {
    let mut body = response.into_body();
    next_sse_frame(&mut body).await
}

async fn next_sse_frame(body: &mut Body) -> String {
    let frame = timeout(Duration::from_secs(1), body.frame())
        .await
        .expect("SSE frame timed out")
        .expect("SSE stream ended")
        .expect("SSE stream failed");
    String::from_utf8(frame.into_data().expect("expected SSE data frame").to_vec()).unwrap()
}

fn event_id(frame: &str) -> &str {
    frame
        .lines()
        .find_map(|line| line.strip_prefix("id: "))
        .expect("SSE frame contains an ID")
}

fn cursor_with_sequence(cursor: &str, sequence: u64) -> String {
    let (epoch, _) = cursor.split_once(':').expect("opaque cursor has an epoch");
    format!("{epoch}:{sequence}")
}

#[tokio::test]
async fn one_worker_calls_the_sdk_and_processes_persistent_jobs_serially() {
    let directory = tempfile::tempdir().unwrap();
    let store = SqliteJobStore::open(directory.path().join("jobs.sqlite"))
        .await
        .unwrap();
    let mut titles = LocalizedValue::new();
    titles.insert("en", "Fixture Movie".to_owned()).unwrap();
    let movie = Movie::new(WorkId::new("fixture-movie").unwrap(), titles);
    let provider = FixtureProvider::new(
        ProviderId::new("fixture.worker").unwrap(),
        [FixtureDocument::new(
            ExternalId::new("fixture.worker", "fixture-movie").unwrap(),
            MetadataDocument::Movie(movie),
        )],
    )
    .unwrap()
    .with_search_delay(Duration::from_millis(150));
    let fixer = Fixer::builder()
        .provider(provider)
        .offline()
        .build()
        .unwrap();
    let runtime = JobRuntime::new(store, capacity(16));
    let _workers = runtime.start_workers(capacity(1), SdkJobFlow::new(fixer));
    let router = job_app(runtime);

    for path in [
        "/media/First Movie (2000).mkv",
        "/media/Second Movie (2001).mkv",
    ] {
        let response = send(
            &router,
            Request::post("/api/v1/jobs")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"media_kind": "movie", "input_path": path, "apply": false}).to_string(),
                ))
                .unwrap(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::ACCEPTED);
    }

    wait_for_state(&router, 1, "searching").await;
    let second = get_job(&router, 2).await;
    assert_eq!(second["job"]["state"], "queued");

    let first = wait_for_state(&router, 1, "awaiting_confirmation").await;
    assert_eq!(first["job"]["review"]["candidate_count"], 1);
    assert_eq!(first["job"]["review"]["conflict_count"], 0);
    assert_eq!(first["job"]["progress"]["stage"], "awaiting_confirmation");

    let second = wait_for_state(&router, 2, "awaiting_confirmation").await;
    assert_eq!(second["job"]["review"]["candidate_count"], 1);
}

#[tokio::test]
async fn two_workers_atomically_claim_distinct_queued_jobs() {
    let directory = tempfile::tempdir().unwrap();
    let store = SqliteJobStore::open(directory.path().join("jobs.sqlite"))
        .await
        .unwrap();
    let runtime = JobRuntime::new(store, capacity(32));
    let _workers = runtime.start_workers(
        capacity(2),
        SdkJobFlow::new(fixture_fixer(Duration::from_millis(200))),
    );
    let router = job_app(runtime);

    create_job(&router, "/media/First Movie.mkv").await;
    create_job(&router, "/media/Second Movie.mkv").await;

    let (first, second) = tokio::join!(
        wait_for_state(&router, 1, "searching"),
        wait_for_state(&router, 2, "searching")
    );
    assert_eq!(first["job"]["state"], "searching");
    assert_eq!(second["job"]["state"], "searching");
}

#[tokio::test]
async fn cancellation_during_sdk_search_prevents_later_worker_stages() {
    let directory = tempfile::tempdir().unwrap();
    let store = SqliteJobStore::open(directory.path().join("jobs.sqlite"))
        .await
        .unwrap();
    let runtime = JobRuntime::new(store, capacity(32));
    let _workers = runtime.start_workers(
        capacity(1),
        SdkJobFlow::new(fixture_fixer(Duration::from_millis(150))),
    );
    let router = job_app(runtime);

    create_job(&router, "/media/Cancelled Movie.mkv").await;
    wait_for_state(&router, 1, "searching").await;
    let response = send(
        &router,
        Request::post("/api/v1/jobs/1/cancel")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response_json(response).await["job"]["state"], "cancelled");

    tokio::time::sleep(Duration::from_millis(200)).await;
    let job = get_job(&router, 1).await;
    assert_eq!(job["job"]["state"], "cancelled");
    assert!(job["job"].get("review").is_none());
}

#[tokio::test]
async fn a_panicking_sdk_provider_interrupts_one_job_and_the_worker_survives() {
    let directory = tempfile::tempdir().unwrap();
    let store = SqliteJobStore::open(directory.path().join("jobs.sqlite"))
        .await
        .unwrap();
    let runtime = JobRuntime::new(store, capacity(32));
    let provider = PanicsOnceProvider {
        inner: fixture_provider(Duration::ZERO),
        panicked: AtomicBool::new(false),
    };
    let fixer = Fixer::builder()
        .provider(provider)
        .offline()
        .build()
        .unwrap();
    let _workers = runtime.start_workers(capacity(1), SdkJobFlow::new(fixer));
    let router = job_app(runtime);

    create_job(&router, "/media/Panic Movie.mkv").await;
    let first = wait_for_state(&router, 1, "interrupted").await;
    assert_eq!(first["job"]["progress"]["stage"], "interrupted");

    create_job(&router, "/media/Recovered Movie.mkv").await;
    let second = wait_for_state(&router, 2, "awaiting_confirmation").await;
    assert_eq!(second["job"]["review"]["candidate_count"], 1);
}

struct PanicsOnceProvider {
    inner: FixtureProvider,
    panicked: AtomicBool,
}

impl Provider for PanicsOnceProvider {
    fn descriptor(&self) -> &ProviderDescriptor {
        self.inner.descriptor()
    }

    fn search<'a>(
        &'a self,
        request: SearchRequest,
        http: &'a dyn HttpClient,
    ) -> BoxFuture<'a, Result<Vec<Candidate>, ProviderError>> {
        assert!(
            self.panicked.swap(true, Ordering::SeqCst),
            "intentional provider panic"
        );
        self.inner.search(request, http)
    }

    fn fetch<'a>(
        &'a self,
        request: FetchRequest,
        http: &'a dyn HttpClient,
    ) -> BoxFuture<'a, Result<MetadataDocument, ProviderError>> {
        self.inner.fetch(request, http)
    }
}

#[tokio::test]
async fn awaited_worker_shutdown_cooperates_after_the_current_sdk_stage() {
    let directory = tempfile::tempdir().unwrap();
    let store = SqliteJobStore::open(directory.path().join("jobs.sqlite"))
        .await
        .unwrap();
    let runtime = JobRuntime::new(store, capacity(32));
    let workers = runtime.start_workers(
        capacity(1),
        SdkJobFlow::new(fixture_fixer(Duration::from_millis(100))),
    );
    let router = job_app(runtime);

    create_job(&router, "/media/Shutdown Movie.mkv").await;
    wait_for_state(&router, 1, "searching").await;
    workers.shutdown().await;

    let job = get_job(&router, 1).await;
    assert_eq!(job["job"]["state"], "interrupted");
    assert_eq!(job["job"]["progress"]["stage"], "interrupted");
}

#[tokio::test]
async fn workers_emit_replayable_progress_and_review_events() {
    let directory = tempfile::tempdir().unwrap();
    let store = SqliteJobStore::open(directory.path().join("jobs.sqlite"))
        .await
        .unwrap();
    let runtime = JobRuntime::new(store, capacity(32));
    let _workers =
        runtime.start_workers(capacity(1), SdkJobFlow::new(fixture_fixer(Duration::ZERO)));
    let router = job_app(runtime);

    create_job(&router, "/media/Event Movie.mkv").await;
    wait_for_state(&router, 1, "awaiting_confirmation").await;
    let response = send(
        &router,
        Request::get("/api/v1/jobs/1/events")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let mut body = response.into_body();
    let mut saw_progress = false;
    let mut saw_review = false;
    for _ in 0..12 {
        let frame = next_sse_frame(&mut body).await;
        saw_progress |= frame.contains("event: progress");
        saw_review |= frame.contains("event: review");
        if saw_progress && saw_review {
            break;
        }
    }
    assert!(saw_progress, "worker SSE omitted progress events");
    assert!(saw_review, "worker SSE omitted review events");
}

fn fixture_provider(delay: Duration) -> FixtureProvider {
    let mut titles = LocalizedValue::new();
    titles.insert("en", "Fixture Movie".to_owned()).unwrap();
    let movie = Movie::new(WorkId::new("fixture-movie").unwrap(), titles);
    FixtureProvider::new(
        ProviderId::new("fixture.worker").unwrap(),
        [FixtureDocument::new(
            ExternalId::new("fixture.worker", "fixture-movie").unwrap(),
            MetadataDocument::Movie(movie),
        )],
    )
    .unwrap()
    .with_search_delay(delay)
}

fn fixture_fixer(delay: Duration) -> Fixer {
    Fixer::builder()
        .provider(fixture_provider(delay))
        .offline()
        .build()
        .unwrap()
}

async fn create_job(router: &Router, path: &str) {
    let response = send(
        router,
        Request::post("/api/v1/jobs")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                json!({"media_kind": "movie", "input_path": path, "apply": false}).to_string(),
            ))
            .unwrap(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::ACCEPTED);
}

async fn get_job(router: &Router, id: i64) -> Value {
    let response = send(
        router,
        Request::get(format!("/api/v1/jobs/{id}"))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    response_json(response).await
}

async fn wait_for_state(router: &Router, id: i64, expected: &str) -> Value {
    timeout(Duration::from_secs(2), async {
        loop {
            let job = get_job(router, id).await;
            if job["job"]["state"] == expected {
                return job;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("job {id} did not reach {expected}"))
}

#[tokio::test]
async fn create_returns_immediately_and_get_reads_the_persistent_job() {
    let app = TestApp::new(8).await;

    let response = app.create_movie_job().await;
    assert_eq!(response.status(), StatusCode::ACCEPTED);
    let body = response_json(response).await;
    assert_eq!(body["schema_version"], 1);
    assert_eq!(body["job"]["id"], 1);
    assert_eq!(body["job"]["state"], "queued");
    assert_eq!(body["job"]["input"]["media_kind"], "movie");
    assert_eq!(body["job"]["input"]["apply"], false);
    assert!(body["job"]["created_at_ms"].as_i64().is_some());

    let response = app
        .request(Request::get("/api/v1/jobs/1").body(Body::empty()).unwrap())
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = response_json(response).await;
    assert_eq!(body["job"]["id"], 1);
    assert_eq!(body["job"]["state"], "queued");
}

#[tokio::test]
async fn cancellation_is_persisted_and_emitted_as_a_replayable_state_event() {
    let app = TestApp::new(8).await;
    assert_eq!(app.create_movie_job().await.status(), StatusCode::ACCEPTED);

    let response = app
        .request(
            Request::get("/api/v1/jobs/1/events")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers()[header::CONTENT_TYPE],
        "text/event-stream"
    );
    let frame = first_sse_frame(response).await;
    let queued_cursor = event_id(&frame).to_owned();
    assert!(queued_cursor.contains(':'));
    assert!(frame.contains("event: state"), "{frame}");
    assert!(frame.contains(r#""state":"queued""#), "{frame}");

    let response = app
        .request(
            Request::post("/api/v1/jobs/1/cancel")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response_json(response).await["job"]["state"], "cancelled");

    let response = app
        .request(
            Request::get("/api/v1/jobs/1/events")
                .header("last-event-id", queued_cursor)
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let frame = first_sse_frame(response).await;
    assert!(frame.contains("event: state"), "{frame}");
    assert!(frame.contains(r#""state":"cancelled""#), "{frame}");

    let response = app
        .request(Request::get("/api/v1/jobs/1").body(Body::empty()).unwrap())
        .await;
    assert_eq!(response_json(response).await["job"]["state"], "cancelled");
}

#[tokio::test]
async fn reconnect_rejects_a_cursor_older_than_the_bounded_history() {
    let app = TestApp::new(1).await;
    assert_eq!(app.create_movie_job().await.status(), StatusCode::ACCEPTED);
    let initial = app
        .request(
            Request::get("/api/v1/jobs/1/events")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    let initial_cursor = event_id(&first_sse_frame(initial).await).to_owned();

    assert_eq!(
        app.request(
            Request::post("/api/v1/jobs/1/cancel")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .status(),
        StatusCode::OK
    );

    let response = app
        .request(
            Request::get("/api/v1/jobs/1/events")
                .header("last-event-id", cursor_with_sequence(&initial_cursor, 0))
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(response.status(), StatusCode::CONFLICT);
    let body = response_json(response).await;
    assert_eq!(body["error"]["code"], "event_history_expired");
}

#[tokio::test]
async fn malformed_requests_and_methods_use_safe_error_envelopes() {
    let app = TestApp::new(8).await;

    for request_value in [
        Request::post("/api/v1/jobs")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from("{"))
            .unwrap(),
        Request::post("/api/v1/jobs")
            .body(Body::from("{}"))
            .unwrap(),
        Request::get("/api/v1/jobs/not-a-number")
            .body(Body::empty())
            .unwrap(),
    ] {
        let response = app.request(request_value).await;
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(
            response_json(response).await["error"]["code"],
            "invalid_input"
        );
    }

    for (method, uri, allow) in [
        (Method::GET, "/api/v1/jobs", "POST"),
        (Method::POST, "/api/v1/jobs/99", "GET, HEAD"),
        (Method::GET, "/api/v1/jobs/99/cancel", "POST"),
        (Method::POST, "/api/v1/jobs/99/events", "GET, HEAD"),
    ] {
        let response = app
            .request(
                Request::builder()
                    .method(method)
                    .uri(uri)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await;
        assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
        assert_eq!(response.headers()[header::ALLOW], allow);
        assert_eq!(
            response_json(response).await["error"]["code"],
            "method_not_allowed"
        );
    }
}

#[tokio::test]
async fn future_and_restarted_runtime_cursors_are_rejected() {
    let app = TestApp::new(8).await;
    assert_eq!(app.create_movie_job().await.status(), StatusCode::ACCEPTED);
    let response = app
        .request(
            Request::get("/api/v1/jobs/1/events")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    let cursor = event_id(&first_sse_frame(response).await).to_owned();

    let response = app
        .request(
            Request::get("/api/v1/jobs/1/events")
                .header("last-event-id", cursor_with_sequence(&cursor, u64::MAX))
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        response_json(response).await["error"]["code"],
        "invalid_input"
    );

    let restarted = app.restarted_router(8);
    let response = send(
        &restarted,
        Request::get("/api/v1/jobs/1/events")
            .header("last-event-id", cursor)
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CONFLICT);
    assert_eq!(
        response_json(response).await["error"]["code"],
        "event_history_expired"
    );
}

#[tokio::test]
async fn a_new_runtime_seeds_persisted_jobs_and_keeps_the_subscription_live() {
    let app = TestApp::new(8).await;
    assert_eq!(app.create_movie_job().await.status(), StatusCode::ACCEPTED);
    let restarted = app.restarted_router(8);

    let response = send(
        &restarted,
        Request::get("/api/v1/jobs/1/events")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let mut body = response.into_body();
    let queued = next_sse_frame(&mut body).await;
    assert!(queued.contains(r#""state":"queued""#), "{queued}");

    let response = send(
        &restarted,
        Request::post("/api/v1/jobs/1/cancel")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response_json(response).await["job"]["state"], "cancelled");

    let cancelled = next_sse_frame(&mut body).await;
    assert!(cancelled.contains(r#""state":"cancelled""#), "{cancelled}");
}
