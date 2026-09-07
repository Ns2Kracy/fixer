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
use fixer_core::{
    ExternalId, LocalizedValue, MetadataDocument, Movie, MovieRelease, ProviderId, ReleaseDate,
    ReleaseId, WorkId,
};
use fixer_sdk::{Fixer, FixtureDocument, FixtureProvider};
use fixer_server::{
    AuthState, FsPolicy, IngestionNotifications, IngestionRuntime, IngestionSupervisor,
    IngestionSupervisorConfig, JobRuntime, SdkJobFlow, SqliteJobStore, WorkspaceState,
    secure_workspace_app_with_notifications,
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sqlx::{Connection, SqliteConnection, sqlite::SqliteConnectOptions};
use tokio::time::{sleep, timeout};
use tower::ServiceExt;

const WAIT: Duration = Duration::from_secs(8);

fn fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/library")
}

fn write_movie(directory: &Path, title: &str) {
    std::fs::create_dir_all(directory).unwrap();
    let nfo =
        std::fs::read_to_string(fixture_root().join("movie/In the Mood for Love (2000)/movie.nfo"))
            .unwrap()
            .replace("In the Mood for Love", title);
    std::fs::write(directory.join("movie.nfo"), nfo).unwrap();
    std::fs::write(directory.join(format!("{title}.mkv")), title.as_bytes()).unwrap();
}

fn fixture_document(id: &str, title: &str) -> FixtureDocument {
    let mut titles = LocalizedValue::new();
    titles.insert("en", title.to_owned()).unwrap();
    let mut movie = Movie::new(WorkId::new(id).unwrap(), titles);
    movie.releases.push(MovieRelease::new(
        ReleaseId::new(format!("{id}-2000")).unwrap(),
        ReleaseDate::year(2000).unwrap(),
    ));
    FixtureDocument::new(
        ExternalId::new("fixture.ingestion", id).unwrap(),
        MetadataDocument::Movie(movie),
    )
}

fn fixture_fixer() -> Fixer {
    let provider = FixtureProvider::new(
        ProviderId::new("fixture.ingestion").unwrap(),
        [
            fixture_document("arrival", "Arrival"),
            fixture_document("moon", "Moon"),
            fixture_document("solaris", "Solaris"),
        ],
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

async fn wait_for_completed(store: &SqliteJobStore, expected: usize) {
    let completed = timeout(WAIT, async {
        loop {
            let jobs = store.list_jobs(100, None).await.unwrap();
            if jobs.len() == expected
                && jobs
                    .iter()
                    .all(|job| job.state() == fixer_server::jobs::model::JobState::Completed)
            {
                return;
            }
            sleep(Duration::from_millis(20)).await;
        }
    })
    .await;
    if completed.is_err() {
        let jobs = store.list_jobs(100, None).await.unwrap();
        let rules = store.list_ingestion_rules(100).await.unwrap();
        panic!("{expected} ingestion jobs did not complete: jobs={jobs:#?}, rules={rules:#?}");
    }
}

async fn wait_for_source_status(database: &Path, relative_path: &str, expected: &str) {
    let options = SqliteConnectOptions::new().filename(database);
    let mut connection = SqliteConnection::connect_with(&options).await.unwrap();
    timeout(WAIT, async {
        loop {
            let status = sqlx::query_scalar::<_, String>(
                "SELECT status FROM ingestion_sources WHERE relative_source_path = ? ORDER BY id DESC LIMIT 1",
            )
            .bind(relative_path)
            .fetch_optional(&mut connection)
            .await
            .unwrap();
            if status.as_deref() == Some(expected) {
                return;
            }
            sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("source {relative_path} did not reach {expected}"));
    connection.close().await.unwrap();
}

fn assert_organized_movie(destination: &Path, title: &str) {
    let package = format!("{title} (2000)");
    let target = destination.join(&package).join(format!("{package}.mkv"));
    assert_eq!(std::fs::read(target).unwrap(), title.as_bytes());
}

async fn create_folder_rule(router: &Router, token: &str) -> i64 {
    let roots = send(
        router,
        Request::get("/api/v1/library/roots")
            .header(header::AUTHORIZATION, format!("Bearer {token}"))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(roots.status(), StatusCode::OK);
    let root_id = response_json(roots).await["roots"][0]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let created = send(
        router,
        Request::post("/api/v1/ingestion-rules")
            .header(header::AUTHORIZATION, format!("Bearer {token}"))
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                json!({
                    "name": "Incoming media",
                    "source": {"root_id": root_id, "path": "incoming"},
                    "destination": {"root_id": root_id, "path": "library"},
                    "media_kind_mode": "auto",
                    "placement": "copy",
                    "path_template_override": null,
                    "enabled": true
                })
                .to_string(),
            ))
            .unwrap(),
    )
    .await;
    assert_eq!(created.status(), StatusCode::CREATED);
    response_json(created).await["rule"]["id"].as_i64().unwrap()
}

#[tokio::test]
async fn authenticated_folder_rule_discovers_organizes_watches_and_deduplicates_after_restart() {
    let root = tempfile::tempdir().unwrap();
    let media_root = root.path().join("media");
    let source = media_root.join("incoming");
    let destination = media_root.join("library");
    let database = root.path().join("ingestion-e2e.sqlite3");
    std::fs::create_dir_all(&source).unwrap();
    std::fs::create_dir_all(&destination).unwrap();
    write_movie(&source.join("Arrival"), "Arrival");
    write_movie(&source.join("Moon"), "Moon");
    let ambiguous = source.join("Shared");
    write_movie(&ambiguous, "Shared");
    std::fs::write(ambiguous.join("Shared.S01E01.mkv"), b"episode").unwrap();

    let store = SqliteJobStore::open(&database).await.unwrap();
    let token = store
        .issue_api_token("ingestion-e2e")
        .await
        .unwrap()
        .token()
        .to_owned();
    let workspace = WorkspaceState::new([&media_root]).unwrap();
    let policy = FsPolicy::new([&media_root]).unwrap();
    let runtime =
        JobRuntime::new(store.clone(), NonZeroUsize::new(64).unwrap()).with_fs_policy(policy);
    let notifications = IngestionNotifications::new();
    let router = secure_workspace_app_with_notifications(
        runtime.clone(),
        AuthState::new(store.clone()),
        workspace.clone(),
        notifications.clone(),
    );

    let rule_id = create_folder_rule(&router, &token).await;

    let workers = runtime.start_workers(
        NonZeroUsize::new(1).unwrap(),
        SdkJobFlow::new(fixture_fixer()),
    );
    let supervisor = IngestionSupervisor::start_with_config(
        IngestionRuntime::new(store.clone(), runtime.clone(), workspace, notifications),
        IngestionSupervisorConfig::new(Duration::from_millis(20), Duration::from_millis(80))
            .with_polling_watcher(Duration::from_millis(20)),
    );

    wait_for_completed(&store, 2).await;
    assert_organized_movie(&destination, "Arrival");
    assert_organized_movie(&destination, "Moon");
    wait_for_source_status(&database, "Shared", "needs_review").await;
    let listed = send(
        &router,
        Request::get("/api/v1/ingestion-rules")
            .header(header::AUTHORIZATION, format!("Bearer {token}"))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(listed.status(), StatusCode::OK);
    assert_eq!(
        response_json(listed).await["rules"][0]["status"],
        "needs_review"
    );

    write_movie(&source.join("Solaris"), "Solaris");
    wait_for_completed(&store, 3).await;
    assert_organized_movie(&destination, "Solaris");

    supervisor.shutdown().await;
    workers.shutdown().await;
    drop(router);
    drop(runtime);
    drop(store);

    let restarted_store = SqliteJobStore::open(&database).await.unwrap();
    let restarted_workspace = WorkspaceState::new([&media_root]).unwrap();
    let restarted_runtime =
        JobRuntime::new(restarted_store.clone(), NonZeroUsize::new(64).unwrap())
            .with_fs_policy(FsPolicy::new([&media_root]).unwrap());
    let restarted_supervisor = IngestionSupervisor::start_with_config(
        IngestionRuntime::new(
            restarted_store.clone(),
            restarted_runtime,
            restarted_workspace,
            IngestionNotifications::new(),
        ),
        IngestionSupervisorConfig::new(Duration::from_millis(20), Duration::from_millis(80))
            .with_polling_watcher(Duration::from_millis(20)),
    );
    sleep(Duration::from_millis(250)).await;

    assert_eq!(restarted_store.list_jobs(100, None).await.unwrap().len(), 3);
    let mut connection =
        SqliteConnection::connect_with(&SqliteConnectOptions::new().filename(&database))
            .await
            .unwrap();
    let source_count =
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM ingestion_sources WHERE rule_id = ?")
            .bind(rule_id)
            .fetch_one(&mut connection)
            .await
            .unwrap();
    assert_eq!(source_count, 4);
    connection.close().await.unwrap();

    restarted_supervisor.shutdown().await;
}
