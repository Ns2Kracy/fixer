use std::num::NonZeroUsize;

use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode, header},
};
use fixer_server::{
    AuthState, IngestionNotification, IngestionNotifications, JobRuntime, SqliteJobStore,
    WorkspaceState, secure_workspace_app_with_notifications,
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tempfile::TempDir;
use tower::ServiceExt;

struct TestApp {
    _root: TempDir,
    root_path: String,
    root_id: String,
    token: String,
    notifications: IngestionNotifications,
    router: Router,
}

impl TestApp {
    async fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let media_root = root.path().join("media");
        std::fs::create_dir(&media_root).unwrap();
        std::fs::create_dir(media_root.join("incoming")).unwrap();
        std::fs::create_dir(media_root.join("incoming/nested")).unwrap();
        std::fs::create_dir(media_root.join("library")).unwrap();
        std::fs::write(media_root.join("incoming/movie.mkv"), b"movie").unwrap();

        let store = SqliteJobStore::open(root.path().join("ingestion.sqlite3"))
            .await
            .unwrap();
        let issued = store.issue_api_token("ingestion-tests").await.unwrap();
        let token = issued.token().to_owned();
        let runtime = JobRuntime::new(store.clone(), NonZeroUsize::new(8).unwrap());
        let auth = AuthState::new(store);
        let workspace = WorkspaceState::new([&media_root]).unwrap();
        let notifications = IngestionNotifications::new();
        let router = secure_workspace_app_with_notifications(
            runtime,
            auth,
            workspace,
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
        assert_eq!(roots.status(), StatusCode::OK);
        let roots = response_json(roots).await;
        let root_id = roots["roots"][0]["id"].as_str().unwrap().to_owned();
        let root_path = media_root.to_string_lossy().into_owned();

        Self {
            _root: root,
            root_path,
            root_id,
            token,
            notifications,
            router,
        }
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

    fn rule_request(&self) -> Value {
        json!({
            "name": "Incoming movies",
            "source": {"root_id": self.root_id, "path": "incoming"},
            "destination": {"root_id": self.root_id, "path": "library"},
            "media_kind_mode": "auto",
            "placement": "copy",
            "path_template_override": "{{ title | sanitize }} ({{ year }})",
            "enabled": true
        })
    }

    fn assert_safe(&self, body: &Value) {
        assert!(!body.to_string().contains(&self.root_path));
    }
}

async fn send(router: &Router, request: Request<Body>) -> axum::response::Response {
    router.clone().oneshot(request).await.unwrap()
}

async fn response_json(response: axum::response::Response) -> Value {
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

#[tokio::test]
async fn one_off_jobs_resolve_directory_references_and_require_safe_pairs() {
    let app = TestApp::new().await;
    let request = json!({
        "media_kind": "movie",
        "source": {"root_id": app.root_id, "path": "incoming"},
        "destination": {"root_id": app.root_id, "path": "library"},
        "placement": "hardlink",
        "apply": true
    });

    let created = app.request("POST", "/api/v1/jobs", Some(request)).await;
    assert_eq!(created.status(), StatusCode::ACCEPTED);
    let created = response_json(created).await;
    assert_eq!(created["job"]["input"]["media_kind"], "movie");
    assert_eq!(created["job"]["input"]["apply"], true);
    assert_eq!(
        created["job"]["input"]["organization"]["placement"],
        "hardlink"
    );
    assert_eq!(
        created["job"]["input"]["organization"]["auto_execute"],
        false
    );
    assert!(created["job"]["input"]["organization"]["origin_rule_id"].is_null());

    let nested = json!({
        "media_kind": "movie",
        "source": {"root_id": app.root_id, "path": "incoming"},
        "destination": {"root_id": app.root_id, "path": "incoming/nested"},
        "placement": "copy",
        "apply": false
    });
    let rejected = app.request("POST", "/api/v1/jobs", Some(nested)).await;
    assert_eq!(rejected.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let rejected = response_json(rejected).await;
    assert_eq!(
        rejected["error"]["details"]["directories"],
        "must identify distinct, non-overlapping directories in configured roots"
    );
}

#[tokio::test]
async fn authenticated_rule_crud_and_scan_use_opaque_directory_references() {
    let app = TestApp::new().await;
    let mut notifications = app.notifications.subscribe();

    let created = app
        .request("POST", "/api/v1/ingestion-rules", Some(app.rule_request()))
        .await;
    assert_eq!(created.status(), StatusCode::CREATED);
    let created = response_json(created).await;
    assert_eq!(
        notifications.try_recv().unwrap(),
        IngestionNotification::Reload
    );
    app.assert_safe(&created);
    assert_eq!(created["rule"]["source"]["root_id"], app.root_id);
    assert_eq!(created["rule"]["source"]["path"], "incoming");
    assert_eq!(created["rule"]["destination"]["path"], "library");
    assert_eq!(created["rule"]["status"], "watching");
    let id = created["rule"]["id"].as_i64().unwrap();

    let listed = app.request("GET", "/api/v1/ingestion-rules", None).await;
    assert_eq!(listed.status(), StatusCode::OK);
    let listed = response_json(listed).await;
    app.assert_safe(&listed);
    assert_eq!(listed["rules"].as_array().unwrap().len(), 1);

    let mut updated_request = app.rule_request();
    updated_request["name"] = json!("Paused movies");
    updated_request["media_kind_mode"] = json!({"fixed": "movie"});
    updated_request["placement"] = json!("hardlink");
    updated_request["path_template_override"] = Value::Null;
    updated_request["enabled"] = json!(false);
    let updated = app
        .request(
            "PUT",
            &format!("/api/v1/ingestion-rules/{id}"),
            Some(updated_request),
        )
        .await;
    assert_eq!(updated.status(), StatusCode::OK);
    let updated = response_json(updated).await;
    assert_eq!(
        notifications.try_recv().unwrap(),
        IngestionNotification::Reload
    );
    app.assert_safe(&updated);
    assert_eq!(updated["rule"]["name"], "Paused movies");
    assert_eq!(updated["rule"]["media_kind_mode"]["fixed"], "movie");
    assert_eq!(updated["rule"]["status"], "paused");

    let scan = app
        .request("POST", &format!("/api/v1/ingestion-rules/{id}/scan"), None)
        .await;
    assert_eq!(scan.status(), StatusCode::ACCEPTED);
    let scan = response_json(scan).await;
    let IngestionNotification::Rescan(rule_id) = notifications.try_recv().unwrap() else {
        panic!("expected a rescan notification");
    };
    assert_eq!(rule_id.get(), id);
    app.assert_safe(&scan);
    assert_eq!(scan["rule_id"], id);

    let deleted = app
        .request("DELETE", &format!("/api/v1/ingestion-rules/{id}"), None)
        .await;
    assert_eq!(deleted.status(), StatusCode::NO_CONTENT);
    assert_eq!(
        notifications.try_recv().unwrap(),
        IngestionNotification::Reload
    );

    let listed = app.request("GET", "/api/v1/ingestion-rules", None).await;
    let listed = response_json(listed).await;
    assert_eq!(listed["rules"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn invalid_rule_directories_placement_and_templates_return_safe_422_errors() {
    let app = TestApp::new().await;

    let mut missing_placement = app.rule_request();
    missing_placement
        .as_object_mut()
        .unwrap()
        .remove("placement");
    let response = app
        .request("POST", "/api/v1/ingestion-rules", Some(missing_placement))
        .await;
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    app.assert_safe(&response_json(response).await);

    let mut invalid_template = app.rule_request();
    invalid_template["path_template_override"] = json!("../{{ title }}");
    let response = app
        .request("POST", "/api/v1/ingestion-rules", Some(invalid_template))
        .await;
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let error = response_json(response).await;
    app.assert_safe(&error);
    assert_eq!(error["error"]["code"], "invalid_template");

    let mut overlap = app.rule_request();
    overlap["destination"]["path"] = json!("incoming/nested");
    let response = app
        .request("POST", "/api/v1/ingestion-rules", Some(overlap))
        .await;
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    app.assert_safe(&response_json(response).await);

    let mut file = app.rule_request();
    file["source"]["path"] = json!("incoming/movie.mkv");
    let response = app
        .request("POST", "/api/v1/ingestion-rules", Some(file))
        .await;
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    app.assert_safe(&response_json(response).await);

    let mut unknown_root = app.rule_request();
    unknown_root["source"]["root_id"] = json!("removed-root");
    let response = app
        .request("POST", "/api/v1/ingestion-rules", Some(unknown_root))
        .await;
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    app.assert_safe(&response_json(response).await);

    std::fs::remove_dir_all(&app.root_path).unwrap();
    let response = app
        .request("POST", "/api/v1/ingestion-rules", Some(app.rule_request()))
        .await;
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    app.assert_safe(&response_json(response).await);
}

#[tokio::test]
async fn unknown_rule_mutations_return_not_found() {
    let app = TestApp::new().await;
    let mut notifications = app.notifications.subscribe();

    let update = app
        .request(
            "PUT",
            "/api/v1/ingestion-rules/999",
            Some(app.rule_request()),
        )
        .await;
    assert_eq!(update.status(), StatusCode::NOT_FOUND);
    app.assert_safe(&response_json(update).await);

    let scan = app
        .request("POST", "/api/v1/ingestion-rules/999/scan", None)
        .await;
    assert_eq!(scan.status(), StatusCode::NOT_FOUND);
    app.assert_safe(&response_json(scan).await);

    let delete = app
        .request("DELETE", "/api/v1/ingestion-rules/999", None)
        .await;
    assert_eq!(delete.status(), StatusCode::NOT_FOUND);
    app.assert_safe(&response_json(delete).await);
    assert!(matches!(
        notifications.try_recv(),
        Err(tokio::sync::broadcast::error::TryRecvError::Empty)
    ));
}
