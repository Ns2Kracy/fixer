use std::str::FromStr;

use fixer_server::{
    SqliteJobStore,
    auth::password::{hash_password, verify_password},
};
use sqlx::{Row, SqlitePool, sqlite::SqliteConnectOptions};

#[test]
fn passwords_use_argon2id_phc_values_and_never_debug_as_plaintext() {
    let password = "correct horse battery staple";
    let encoded = hash_password(password).unwrap();

    assert!(encoded.as_str().starts_with("$argon2id$v=19$"));
    assert!(verify_password(password, &encoded).unwrap());
    assert!(!verify_password("wrong password", &encoded).unwrap());
    assert!(!format!("{encoded:?}").contains(password));
    assert!(!format!("{encoded:?}").contains(encoded.as_str()));
    assert!(hash_password("").is_err());
}

#[tokio::test]
async fn api_tokens_are_shown_once_but_only_sha256_digests_are_persisted() {
    let root = tempfile::tempdir().unwrap();
    let database = root.path().join("auth.sqlite3");
    let store = SqliteJobStore::open(&database).await.unwrap();

    let issued = store.issue_api_token("automation").await.unwrap();
    let presented = issued.token().to_owned();
    assert!(presented.starts_with("fixer_pat_"));
    assert!(presented.len() >= "fixer_pat_".len() + 43);
    assert!(!format!("{issued:?}").contains(&presented));
    assert_eq!(
        store.authenticate_api_token(&presented).await.unwrap(),
        Some(issued.id())
    );
    assert_eq!(
        store
            .authenticate_api_token("fixer_pat_not-a-real-secret")
            .await
            .unwrap(),
        None
    );

    let pool = SqlitePool::connect_with(
        SqliteConnectOptions::from_str(database.to_str().unwrap())
            .unwrap()
            .foreign_keys(true),
    )
    .await
    .unwrap();
    let row = sqlx::query("SELECT name, token_digest FROM api_tokens WHERE id = ?")
        .bind(issued.id())
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(row.get::<String, _>("name"), "automation");
    let digest = row.get::<Vec<u8>, _>("token_digest");
    assert_eq!(digest.len(), 32);
    assert_ne!(digest, presented.as_bytes());
    let schema = sqlx::query("SELECT sql FROM sqlite_schema WHERE name = 'api_tokens'")
        .fetch_one(&pool)
        .await
        .unwrap()
        .get::<String, _>("sql");
    assert!(!schema.contains("token TEXT"));

    assert!(store.revoke_api_token(issued.id()).await.unwrap());
    assert_eq!(
        store.authenticate_api_token(&presented).await.unwrap(),
        None
    );
    pool.close().await;
}

#[tokio::test]
async fn password_login_issues_expiring_digest_only_sessions_with_csrf_secrets() {
    let root = tempfile::tempdir().unwrap();
    let database = root.path().join("sessions.sqlite3");
    let store = SqliteJobStore::open(&database).await.unwrap();
    let encoded = hash_password("session password").unwrap();
    store.set_password_hash(&encoded).await.unwrap();

    assert!(
        store
            .verify_single_user_password("session password")
            .await
            .unwrap()
    );
    assert!(
        !store
            .verify_single_user_password("wrong password")
            .await
            .unwrap()
    );
    let issued = store
        .create_session(std::time::Duration::from_secs(60))
        .await
        .unwrap();
    assert!(issued.token().starts_with("fixer_session_"));
    assert!(issued.csrf_token().starts_with("fixer_csrf_"));
    assert!(!format!("{issued:?}").contains(issued.token()));
    assert!(
        store
            .authenticate_session(issued.token(), Some(issued.csrf_token()))
            .await
            .unwrap()
    );
    assert!(
        !store
            .authenticate_session(issued.token(), Some("wrong csrf"))
            .await
            .unwrap()
    );
    assert!(store.revoke_session(issued.token()).await.unwrap());
    assert!(
        !store
            .authenticate_session(issued.token(), None)
            .await
            .unwrap()
    );

    let pool = SqlitePool::connect_with(
        SqliteConnectOptions::from_str(database.to_str().unwrap())
            .unwrap()
            .foreign_keys(true),
    )
    .await
    .unwrap();
    let password: String = sqlx::query_scalar("SELECT password_hash FROM single_user_auth")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(password, encoded.as_str());
    let session_columns = sqlx::query("PRAGMA table_info(auth_sessions)")
        .fetch_all(&pool)
        .await
        .unwrap()
        .into_iter()
        .map(|row| row.get::<String, _>("name"))
        .collect::<Vec<_>>();
    assert_eq!(
        session_columns,
        [
            "token_digest",
            "csrf_digest",
            "created_at_ms",
            "expires_at_ms"
        ]
    );
    pool.close().await;
}

use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use fixer_server::{AuthState, JobRuntime, WorkspaceState, secure_workspace_app};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use std::num::NonZeroUsize;
use tower::ServiceExt;

async fn secure_app(secure_cookie: bool) -> (tempfile::TempDir, SqliteJobStore, axum::Router) {
    let root = tempfile::tempdir().unwrap();
    let store = SqliteJobStore::open(root.path().join("http-auth.sqlite3"))
        .await
        .unwrap();
    let password = hash_password("web password").unwrap();
    store.set_password_hash(&password).await.unwrap();
    let runtime = JobRuntime::new(store.clone(), NonZeroUsize::new(8).unwrap());
    let auth = AuthState::new(store.clone())
        .with_secure_cookie(secure_cookie)
        .with_allowed_origins(["https://fixer.example"])
        .unwrap();
    let workspace = WorkspaceState::new([root.path()]).unwrap();
    (root, store, secure_workspace_app(runtime, auth, workspace))
}

async fn json_body(response: axum::response::Response) -> Value {
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

async fn login(router: &axum::Router) -> (String, String) {
    let response = router
        .clone()
        .oneshot(
            Request::post("/api/v1/auth/login")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({"password": "web password"}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let set_cookie = response.headers()[header::SET_COOKIE]
        .to_str()
        .unwrap()
        .to_owned();
    let cookie = set_cookie.split(';').next().unwrap().to_owned();
    let body = json_body(response).await;
    (cookie, body["csrf_token"].as_str().unwrap().to_owned())
}

#[tokio::test]
async fn login_sets_strict_http_only_cookie_and_protects_api_routes() {
    let (_root, _store, router) = secure_app(true).await;
    let unauthorized = router
        .clone()
        .oneshot(
            Request::get("/api/v1/providers")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(unauthorized.status(), StatusCode::UNAUTHORIZED);

    let wrong = router
        .clone()
        .oneshot(
            Request::post("/api/v1/auth/login")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({"password": "wrong"}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(wrong.status(), StatusCode::UNAUTHORIZED);

    let response = router
        .clone()
        .oneshot(
            Request::post("/api/v1/auth/login")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({"password": "web password"}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let set_cookie = response.headers()[header::SET_COOKIE]
        .to_str()
        .unwrap()
        .to_owned();
    assert!(set_cookie.contains("HttpOnly"));
    assert!(set_cookie.contains("SameSite=Strict"));
    assert!(set_cookie.contains("Secure"));
    assert!(set_cookie.contains("Path=/api"));
    let cookie = set_cookie.split(';').next().unwrap().to_owned();
    let body = json_body(response).await;
    assert!(
        body["csrf_token"]
            .as_str()
            .unwrap()
            .starts_with("fixer_csrf_")
    );

    let authorized = router
        .oneshot(
            Request::get("/api/v1/providers")
                .header(header::COOKIE, cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(authorized.status(), StatusCode::OK);
}

#[tokio::test]
async fn workspace_routes_require_authentication_and_csrf() {
    let (_root, _store, router) = secure_app(false).await;

    let unauthorized = router
        .clone()
        .oneshot(
            Request::get("/api/v1/settings")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(unauthorized.status(), StatusCode::UNAUTHORIZED);

    let (cookie, csrf) = login(&router).await;
    let settings = router
        .clone()
        .oneshot(
            Request::get("/api/v1/settings")
                .header(header::COOKIE, &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(settings.status(), StatusCode::OK);

    let missing_csrf = router
        .clone()
        .oneshot(
            Request::post("/api/v1/providers/local/test")
                .header(header::COOKIE, &cookie)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(missing_csrf.status(), StatusCode::FORBIDDEN);

    let accepted = router
        .oneshot(
            Request::post("/api/v1/providers/local/test")
                .header(header::COOKIE, cookie)
                .header("x-csrf-token", csrf)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(accepted.status(), StatusCode::OK);
}

#[tokio::test]
async fn cookie_state_changes_require_csrf_but_bearer_tokens_do_not() {
    let (root, store, router) = secure_app(false).await;
    let media = root.path().join("movie.mkv");
    std::fs::write(&media, b"movie").unwrap();
    let (cookie, csrf) = login(&router).await;
    let body = json!({"media_kind": "movie", "input_path": media, "apply": false}).to_string();

    let missing = router
        .clone()
        .oneshot(
            Request::post("/api/v1/jobs")
                .header(header::COOKIE, &cookie)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(body.clone()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(missing.status(), StatusCode::FORBIDDEN);

    let accepted = router
        .clone()
        .oneshot(
            Request::post("/api/v1/jobs")
                .header(header::COOKIE, &cookie)
                .header("x-csrf-token", &csrf)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(body.clone()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(accepted.status(), StatusCode::ACCEPTED);

    let issued = store.issue_api_token("test client").await.unwrap();
    let bearer = router
        .oneshot(
            Request::post("/api/v1/jobs")
                .header(header::AUTHORIZATION, format!("Bearer {}", issued.token()))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(bearer.status(), StatusCode::ACCEPTED);
}

#[tokio::test]
async fn logout_revokes_session_and_cors_allows_only_exact_origins() {
    let (_root, _store, router) = secure_app(false).await;
    let (cookie, csrf) = login(&router).await;

    let disallowed = router
        .clone()
        .oneshot(
            Request::get("/api/v1/health")
                .header(header::ORIGIN, "https://evil.example")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(disallowed.status(), StatusCode::FORBIDDEN);

    let allowed = router
        .clone()
        .oneshot(
            Request::get("/api/v1/health")
                .header(header::ORIGIN, "https://fixer.example")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(allowed.status(), StatusCode::OK);
    assert_eq!(
        allowed.headers()[header::ACCESS_CONTROL_ALLOW_ORIGIN],
        "https://fixer.example"
    );
    assert_eq!(
        allowed.headers()[header::ACCESS_CONTROL_ALLOW_CREDENTIALS],
        "true"
    );

    let logout = router
        .clone()
        .oneshot(
            Request::post("/api/v1/auth/logout")
                .header(header::COOKIE, &cookie)
                .header("x-csrf-token", csrf)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(logout.status(), StatusCode::NO_CONTENT);
    assert!(
        logout.headers()[header::SET_COOKIE]
            .to_str()
            .unwrap()
            .contains("Max-Age=0")
    );

    let revoked = router
        .oneshot(
            Request::get("/api/v1/providers")
                .header(header::COOKIE, cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(revoked.status(), StatusCode::UNAUTHORIZED);
}
