use std::{
    num::NonZeroUsize,
    path::{Path, PathBuf},
    time::Duration,
};

use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode, header},
};
use fixer_server::{
    AuthState, IngestionNotifications, IngestionRuntime, IngestionSupervisor,
    IngestionSupervisorConfig, JobRuntime, SqliteJobStore, WorkspaceState,
    secure_workspace_app_with_notifications,
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tempfile::TempDir;
use tokio::time::{sleep, timeout};
use tower::ServiceExt;

struct Harness {
    _root: TempDir,
    source: PathBuf,
    destination: PathBuf,
    root_id: String,
    token: String,
    store: SqliteJobStore,
    jobs: JobRuntime,
    workspace: WorkspaceState,
    notifications: IngestionNotifications,
    router: Router,
}

impl Harness {
    async fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let media_root = root.path().join("media");
        let source = media_root.join("incoming");
        let destination = media_root.join("library");
        std::fs::create_dir_all(&source).unwrap();
        std::fs::create_dir_all(&destination).unwrap();

        let store = SqliteJobStore::open(root.path().join("watch.sqlite3"))
            .await
            .unwrap();
        let issued = store.issue_api_token("watch-tests").await.unwrap();
        let token = issued.token().to_owned();
        let jobs = JobRuntime::new(store.clone(), NonZeroUsize::new(8).unwrap());
        let workspace = WorkspaceState::new([&media_root]).unwrap();
        let notifications = IngestionNotifications::new();
        let router = secure_workspace_app_with_notifications(
            jobs.clone(),
            AuthState::new(store.clone()),
            workspace.clone(),
            notifications.clone(),
        );
        let roots = send(
            &router,
            Request::get("/api/v1/library/roots")
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        let roots = response_json(roots).await;
        let root_id = roots["roots"][0]["id"].as_str().unwrap().to_owned();

        Self {
            _root: root,
            source,
            destination,
            root_id,
            token,
            store,
            jobs,
            workspace,
            notifications,
            router,
        }
    }

    fn start(&self, reconciliation: Duration) -> IngestionSupervisor {
        IngestionSupervisor::start_with_config(
            IngestionRuntime::new(
                self.store.clone(),
                self.jobs.clone(),
                self.workspace.clone(),
                self.notifications.clone(),
            ),
            IngestionSupervisorConfig::new(Duration::from_millis(20), reconciliation),
        )
    }

    async fn create_rule(&self, enabled: bool) -> i64 {
        let response = self
            .request(
                "POST",
                "/api/v1/ingestion-rules",
                Some(json!({
                    "name": "Watched movies",
                    "source": {"root_id": self.root_id, "path": "incoming"},
                    "destination": {"root_id": self.root_id, "path": "library"},
                    "media_kind_mode": {"fixed": "movie"},
                    "placement": "copy",
                    "path_template_override": "{{ title | sanitize }} ({{ year }})",
                    "enabled": enabled
                })),
            )
            .await;
        assert_eq!(response.status(), StatusCode::CREATED);
        response_json(response).await["rule"]["id"]
            .as_i64()
            .unwrap()
    }

    async fn request(
        &self,
        method: &str,
        uri: &str,
        body: Option<Value>,
    ) -> axum::response::Response {
        let mut builder = Request::builder()
            .method(method)
            .uri(uri)
            .header(header::AUTHORIZATION, format!("Bearer {}", self.token));
        let body = if let Some(body) = body {
            builder = builder.header(header::CONTENT_TYPE, "application/json");
            Body::from(body.to_string())
        } else {
            Body::empty()
        };
        send(&self.router, builder.body(body).unwrap()).await
    }

    async fn wait_for_jobs(&self, expected: usize) {
        timeout(Duration::from_secs(4), async {
            loop {
                if self.store.list_jobs(100, None).await.unwrap().len() == expected {
                    return;
                }
                sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap_or_else(|_| panic!("timed out waiting for {expected} ingestion jobs"));
    }
}

fn write_movie(root: &Path, title: &str) {
    std::fs::create_dir_all(root).unwrap();
    std::fs::write(
        root.join("movie.nfo"),
        format!(
            "<movie><title>{title}</title><year>2024</year><uniqueid type=\"tmdb\">1234</uniqueid></movie>"
        ),
    )
    .unwrap();
}

async fn send(router: &Router, request: Request<Body>) -> axum::response::Response {
    router.clone().oneshot(request).await.unwrap()
}

async fn response_json(response: axum::response::Response) -> Value {
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

#[tokio::test]
async fn recursive_reconciliation_deduplicates_and_reprocesses_changed_fingerprints() {
    let app = Harness::new().await;
    let movie = app.source.join("nested/Arrival (2016)");
    write_movie(&movie, "Arrival");
    let supervisor = app.start(Duration::from_millis(80));
    let rule_id = app.create_rule(true).await;

    app.wait_for_jobs(1).await;
    sleep(Duration::from_millis(180)).await;
    assert_eq!(app.store.list_jobs(100, None).await.unwrap().len(), 1);

    write_movie(&movie, "Arrival Extended");
    app.wait_for_jobs(2).await;
    let jobs = app.store.list_jobs(100, None).await.unwrap();
    assert!(jobs.iter().all(|job| {
        let organization = job.input().organization().unwrap();
        !job.input().apply()
            && organization.origin_rule_id == Some(rule_id)
            && !organization.auto_execute
            && organization.destination_path
                == app.destination.canonicalize().unwrap().to_string_lossy()
    }));

    write_movie(
        &app.destination.join("Destination Only"),
        "Destination Only",
    );
    std::fs::write(app.source.join("download.part"), b"partial").unwrap();
    sleep(Duration::from_millis(180)).await;
    assert_eq!(app.store.list_jobs(100, None).await.unwrap().len(), 2);

    supervisor.shutdown().await;
}

#[tokio::test]
async fn disabled_rules_never_enqueue_jobs() {
    let app = Harness::new().await;
    write_movie(&app.source.join("Disabled Movie"), "Disabled Movie");
    app.create_rule(false).await;
    let supervisor = app.start(Duration::from_millis(50));

    sleep(Duration::from_millis(180)).await;
    assert!(app.store.list_jobs(100, None).await.unwrap().is_empty());

    supervisor.shutdown().await;
}

#[tokio::test]
async fn missing_directories_recover_and_explicit_rescan_remains_deduplicated() {
    let app = Harness::new().await;
    let rule_id = app.create_rule(true).await;
    std::fs::remove_dir_all(&app.source).unwrap();
    let supervisor = app.start(Duration::from_millis(50));

    timeout(Duration::from_secs(2), async {
        loop {
            let rules = app.store.list_ingestion_rules(100).await.unwrap();
            if rules[0].last_error().is_some() {
                break;
            }
            sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("missing source did not record a rule error");

    write_movie(&app.source.join("Recovered Movie"), "Recovered Movie");
    app.wait_for_jobs(1).await;
    timeout(Duration::from_secs(2), async {
        loop {
            if app.store.list_ingestion_rules(100).await.unwrap()[0]
                .last_error()
                .is_none()
            {
                break;
            }
            sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("recovered source did not clear its rule error");

    let response = app
        .request(
            "POST",
            &format!("/api/v1/ingestion-rules/{rule_id}/scan"),
            None,
        )
        .await;
    assert_eq!(response.status(), StatusCode::ACCEPTED);
    sleep(Duration::from_millis(120)).await;
    assert_eq!(app.store.list_jobs(100, None).await.unwrap().len(), 1);

    supervisor.shutdown().await;
}
