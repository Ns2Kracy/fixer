use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode, header},
    routing::head,
};
use fixer_runtime::ConfigLoader;
use fixer_server::{WorkspaceState, workspace_app};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tempfile::TempDir;
use tower::ServiceExt;

async fn response_json(response: axum::response::Response) -> Value {
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

fn put_json(uri: &str, value: &Value) -> Request<Body> {
    Request::put(uri)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(value.to_string()))
        .unwrap()
}

fn post_json(uri: &str, value: &Value) -> Request<Body> {
    Request::post(uri)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(value.to_string()))
        .unwrap()
}

fn settings(endpoint: &str) -> Value {
    json!({
        "offline": false,
        "proxy": null,
        "preferred_locales": ["zh-Hans", "ja", "en", "und"],
        "timeout_seconds": 5,
        "auto_accept_confidence": 0.9,
        "review_confidence": 0.6,
        "output_preset": "metadata",
        "placement": "reflink",
        "conflict_policy": "review",
        "enabled_providers": ["local", "tmdb", "bangumi"],
        "provider_endpoints": {
            "tmdb": endpoint,
            "bangumi": endpoint,
            "anilist": endpoint,
            "musicbrainz": endpoint,
            "openlibrary": endpoint,
            "openlibrary_cover": format!("{endpoint}/covers/")
        },
        "tmdb_api_token": "write-only-tmdb-secret",
        "anilist_access_token": "write-only-anilist-secret",
        "clear_tmdb_api_token": false,
        "clear_anilist_access_token": false
    })
}

#[tokio::test]
async fn settings_are_validated_and_secrets_are_write_only() {
    let app = workspace_app(WorkspaceState::default());
    let update = app
        .clone()
        .oneshot(put_json(
            "/api/v1/settings",
            &settings("http://127.0.0.1:9"),
        ))
        .await
        .unwrap();

    assert_eq!(update.status(), StatusCode::OK);
    let updated = response_json(update).await;
    assert_eq!(updated["schema_version"], 1);
    assert_eq!(
        updated["settings"]["preferred_locales"],
        json!(["zh-Hans", "ja", "en", "und"])
    );
    assert_eq!(updated["settings"]["output_preset"], "metadata");
    assert_eq!(updated["settings"]["placement"], "reflink");
    assert_eq!(
        updated["settings"]["secrets"],
        json!({
            "tmdb_api_token_configured": true,
            "anilist_access_token_configured": true
        })
    );
    let serialized = updated.to_string();
    assert!(!serialized.contains("write-only-tmdb-secret"));
    assert!(!serialized.contains("write-only-anilist-secret"));

    let fetched = app
        .clone()
        .oneshot(
            Request::get("/api/v1/settings")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let fetched = response_json(fetched).await;
    assert_eq!(fetched, updated);
    assert!(!fetched.to_string().contains("write-only"));

    let mut invalid = settings("http://127.0.0.1:9");
    invalid["review_confidence"] = json!(0.95);
    let response = app
        .clone()
        .oneshot(put_json("/api/v1/settings", &invalid))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let error = response_json(response).await;
    assert_eq!(error["error"]["code"], "invalid_input");
    assert_eq!(
        error["error"]["details"]["review_confidence"],
        "must not exceed auto_accept_confidence"
    );

    let mut credentialed_proxy = settings("http://127.0.0.1:9");
    credentialed_proxy["proxy"] = json!("socks5://user:password@127.0.0.1:1080");
    let response = app
        .oneshot(put_json("/api/v1/settings", &credentialed_proxy))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let error = response_json(response).await;
    assert_eq!(
        error["error"]["details"]["proxy"],
        "must not contain credentials"
    );
    assert!(!error.to_string().contains("password"));
}

#[tokio::test]
#[allow(
    clippy::too_many_lines,
    reason = "the persistence test keeps one complete save-and-reload scenario together"
)]
async fn settings_persist_through_config_handle_and_survive_restart() {
    let root = tempfile::tempdir().unwrap();
    let media = root.path().join("media");
    std::fs::create_dir(&media).unwrap();
    std::fs::write(
        root.path().join("fixer.toml"),
        r#"
enabled_providers = ["local", "tmdb"]

[providers.tmdb]
api_token_env = "TMDB_SECRET"

[providers.anilist]
access_token = "direct-anilist-secret"

[server]
bind = "127.0.0.1:4100"
media_roots = ["media"]
worker_count = 3
"#,
    )
    .unwrap();
    let environment = std::collections::BTreeMap::from([(
        "TMDB_SECRET".to_owned(),
        "environment-secret".to_owned(),
    )]);
    let handle = ConfigLoader::new(root.path())
        .with_environment(environment.clone())
        .load()
        .unwrap()
        .into_handle();
    let app = workspace_app(WorkspaceState::new_with_config([&media], handle.clone()).unwrap());

    let fetched = app
        .clone()
        .oneshot(
            Request::get("/api/v1/settings")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let fetched = response_json(fetched).await;
    assert_eq!(
        fetched["settings"]["enabled_providers"],
        json!(["local", "tmdb"])
    );
    assert_eq!(
        fetched["settings"]["secrets"],
        json!({
            "tmdb_api_token_configured": true,
            "anilist_access_token_configured": true,
            "tmdb_api_token_env": "TMDB_SECRET"
        })
    );
    assert!(!fetched.to_string().contains("environment-secret"));
    assert!(!fetched.to_string().contains("direct-anilist-secret"));

    let mut request = settings("http://127.0.0.1:9");
    request["tmdb_api_token"] = Value::Null;
    request["anilist_access_token"] = Value::Null;
    let update = app
        .clone()
        .oneshot(put_json("/api/v1/settings", &request))
        .await
        .unwrap();
    assert_eq!(update.status(), StatusCode::OK);
    let updated = response_json(update).await;
    assert_eq!(
        updated["settings"]["secrets"]["tmdb_api_token_env"],
        "TMDB_SECRET"
    );
    assert!(!updated.to_string().contains("environment-secret"));
    assert!(!updated.to_string().contains("write-only-tmdb-secret"));

    let persisted = std::fs::read_to_string(root.path().join("fixer.toml")).unwrap();
    assert!(persisted.contains("output_preset = \"metadata\""));
    assert!(persisted.contains("api_token_env = \"TMDB_SECRET\""));
    let reloaded = ConfigLoader::new(root.path())
        .with_environment(environment)
        .load()
        .unwrap();
    assert_eq!(reloaded.config().output_preset.to_string(), "metadata");
    assert_eq!(
        reloaded.config().providers.tmdb.base_url,
        "http://127.0.0.1:9"
    );
    assert_eq!(reloaded.config().server.bind.to_string(), "127.0.0.1:4100");
    assert_eq!(reloaded.config().server.worker_count, 3);
    assert_eq!(
        reloaded.config().server.media_roots,
        vec![media.canonicalize().unwrap()]
    );

    let before_file = std::fs::read(root.path().join("fixer.toml")).unwrap();
    let before_memory = handle.snapshot();
    let mut forbidden = settings("http://127.0.0.1:9");
    forbidden["server"] = json!({"bind": "0.0.0.0:9999", "worker_count": 99});
    let response = app
        .clone()
        .oneshot(put_json("/api/v1/settings", &forbidden))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        std::fs::read(root.path().join("fixer.toml")).unwrap(),
        before_file
    );
    assert_eq!(handle.snapshot(), before_memory);

    let mut invalid = settings("http://127.0.0.1:9");
    invalid["review_confidence"] = json!(0.95);
    let response = app
        .oneshot(put_json("/api/v1/settings", &invalid))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        std::fs::read(root.path().join("fixer.toml")).unwrap(),
        before_file
    );
    assert_eq!(handle.snapshot(), before_memory);
}

#[tokio::test]
async fn direct_secret_replacement_and_clear_update_reference_metadata() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(
        root.path().join("fixer.toml"),
        "[providers.tmdb]\napi_token_env = \"TMDB_SECRET\"\n",
    )
    .unwrap();
    let handle = ConfigLoader::new(root.path())
        .with_environment(std::collections::BTreeMap::from([(
            "TMDB_SECRET".to_owned(),
            "environment-secret".to_owned(),
        )]))
        .load()
        .unwrap()
        .into_handle();
    let app = workspace_app(
        WorkspaceState::new_with_config(std::iter::empty::<&str>(), handle.clone()).unwrap(),
    );

    let mut replacement = settings("http://127.0.0.1:9");
    replacement["tmdb_api_token"] = json!("replacement-secret");
    replacement["anilist_access_token"] = Value::Null;
    let response = app
        .clone()
        .oneshot(put_json("/api/v1/settings", &replacement))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = response_json(response).await;
    assert_eq!(
        body["settings"]["secrets"]["tmdb_api_token_configured"],
        true
    );
    assert!(
        body["settings"]["secrets"]
            .get("tmdb_api_token_env")
            .is_none()
    );
    assert!(!body.to_string().contains("replacement-secret"));
    let replaced = handle.snapshot();
    assert_eq!(
        replaced
            .providers
            .tmdb
            .api_token
            .as_ref()
            .unwrap()
            .expose_secret(),
        "replacement-secret"
    );
    assert!(replaced.providers.tmdb.api_token_env.is_none());

    let mut clear = settings("http://127.0.0.1:9");
    clear["tmdb_api_token"] = Value::Null;
    clear["anilist_access_token"] = Value::Null;
    clear["clear_tmdb_api_token"] = json!(true);
    let response = app
        .oneshot(put_json("/api/v1/settings", &clear))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = response_json(response).await;
    assert_eq!(
        body["settings"]["secrets"]["tmdb_api_token_configured"],
        false
    );
    let cleared = handle.snapshot();
    assert!(cleared.providers.tmdb.api_token.is_none());
    assert!(cleared.providers.tmdb.api_token_env.is_none());
    assert!(
        !std::fs::read_to_string(root.path().join("fixer.toml"))
            .unwrap()
            .contains("replacement-secret")
    );
}

#[tokio::test]
async fn settings_persistence_failure_keeps_shared_memory_unchanged() {
    let root = tempfile::tempdir().unwrap();
    let config_dir = root.path().join("config");
    let config_path = config_dir.join("fixer.toml");
    std::fs::create_dir(&config_dir).unwrap();
    std::fs::write(&config_path, "offline = false\n").unwrap();
    let handle = ConfigLoader::new(root.path())
        .with_config_path(&config_path)
        .load()
        .unwrap()
        .into_handle();
    let before = handle.snapshot();
    let app = workspace_app(
        WorkspaceState::new_with_config(std::iter::empty::<&str>(), handle.clone()).unwrap(),
    );

    std::fs::remove_file(&config_path).unwrap();
    std::fs::remove_dir(&config_dir).unwrap();
    std::fs::write(&config_dir, b"blocker").unwrap();

    let response = app
        .oneshot(put_json(
            "/api/v1/settings",
            &settings("http://127.0.0.1:9"),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    let error = response_json(response).await;
    assert_eq!(error["error"]["code"], "workspace_error");
    assert_eq!(handle.snapshot(), before);
    assert_eq!(std::fs::read(&config_dir).unwrap(), b"blocker");
}

#[tokio::test]
async fn library_and_search_accept_only_opaque_configured_roots() {
    let root = TempDir::new().unwrap();
    std::fs::create_dir(root.path().join("Books")).unwrap();
    std::fs::write(root.path().join("Books/Fixture Book.epub"), b"fixture").unwrap();
    std::fs::write(root.path().join("Fixture Movie.mkv"), b"fixture").unwrap();
    let app = workspace_app(WorkspaceState::new([root.path()]).unwrap());

    let roots = app
        .clone()
        .oneshot(
            Request::get("/api/v1/library/roots")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(roots.status(), StatusCode::OK);
    let roots = response_json(roots).await;
    assert_eq!(roots["roots"][0]["id"], "root-0");
    assert!(
        roots["roots"][0]["label"]
            .as_str()
            .is_some_and(|label| !label.is_empty())
    );

    let listing = app
        .clone()
        .oneshot(
            Request::get("/api/v1/library?root_id=root-0&path=Books")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(listing.status(), StatusCode::OK);
    let listing = response_json(listing).await;
    assert_eq!(listing["root_id"], "root-0");
    assert_eq!(listing["path"], "Books");
    assert_eq!(listing["entries"][0]["path"], "Books/Fixture Book.epub");

    for media_kind in ["movie", "television", "anime", "music", "book"] {
        let response = app
            .clone()
            .oneshot(
                Request::get(format!(
                    "/api/v1/search?media_kind={media_kind}&query=fixture&limit=10"
                ))
                .body(Body::empty())
                .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = response_json(response).await;
        assert_eq!(body["media_kind"], media_kind);
        assert!(
            body["results"]
                .as_array()
                .is_some_and(|results| !results.is_empty())
        );
        assert!(
            body["results"]
                .as_array()
                .unwrap()
                .iter()
                .all(|result| result["root_id"] == "root-0")
        );
    }

    for uri in [
        "/api/v1/library?root_id=root-0&path=..%2Foutside",
        "/api/v1/library?root_id=root-0&path=%2Ftmp",
        "/api/v1/library?root_id=missing&path=",
    ] {
        let response = app
            .clone()
            .oneshot(Request::get(uri).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert!(matches!(
            response.status(),
            StatusCode::BAD_REQUEST | StatusCode::NOT_FOUND
        ));
        let body = response_json(response).await;
        assert!(
            body["error"]["request_id"]
                .as_str()
                .is_some_and(|id| !id.is_empty())
        );
    }
}

#[tokio::test]
async fn provider_connectivity_returns_precise_safe_categories() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let fixture = Router::new().route("/", head(|| async { StatusCode::NO_CONTENT }));
    let server = tokio::spawn(async move { axum::serve(listener, fixture).await.unwrap() });
    let endpoint = format!("http://{address}");
    let app = workspace_app(WorkspaceState::default());

    let update = app
        .clone()
        .oneshot(put_json("/api/v1/settings", &settings(&endpoint)))
        .await
        .unwrap();
    assert_eq!(update.status(), StatusCode::OK);

    let ready = app
        .clone()
        .oneshot(post_json("/api/v1/providers/tmdb/test", &json!({})))
        .await
        .unwrap();
    assert_eq!(ready.status(), StatusCode::OK);
    assert_eq!(
        response_json(ready).await,
        json!({
            "schema_version": 1,
            "provider": "tmdb",
            "ok": true,
            "category": "ready",
            "message": "Provider endpoint is reachable"
        })
    );

    let disabled = app
        .clone()
        .oneshot(post_json("/api/v1/providers/anilist/test", &json!({})))
        .await
        .unwrap();
    assert_eq!(disabled.status(), StatusCode::OK);
    let disabled = response_json(disabled).await;
    assert_eq!(disabled["ok"], false);
    assert_eq!(disabled["category"], "disabled");

    let unknown = app
        .oneshot(post_json("/api/v1/providers/not-real/test", &json!({})))
        .await
        .unwrap();
    assert_eq!(unknown.status(), StatusCode::NOT_FOUND);
    let unknown = response_json(unknown).await;
    assert_eq!(unknown["error"]["code"], "provider_not_found");
    assert!(!unknown.to_string().contains(&endpoint));

    server.abort();
}

#[tokio::test]
async fn template_preview_validates_and_renders_without_writing() {
    let root = TempDir::new().unwrap();
    let app = workspace_app(WorkspaceState::new([root.path()]).unwrap());
    let request = json!({
        "path_template": "{{title|sanitize}} ({{year}})/metadata.json",
        "content_template": "title={{title}}\nid={{id}}",
        "sample": {
            "title": "Fixture: Movie",
            "id": "fixture-movie",
            "year": 2024,
            "edition": null
        }
    });

    let response = app
        .clone()
        .oneshot(post_json("/api/v1/templates/preview", &request))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response_json(response).await,
        json!({
            "schema_version": 1,
            "path": "Fixture Movie (2024)/metadata.json",
            "content": "title=Fixture: Movie\nid=fixture-movie",
            "content_bytes": 37
        })
    );
    assert!(std::fs::read_dir(root.path()).unwrap().next().is_none());

    let invalid = app
        .oneshot(post_json(
            "/api/v1/templates/preview",
            &json!({
                "path_template": "../{{title}}",
                "content_template": "{{unknown}}",
                "sample": {"title": "Fixture", "id": "fixture", "year": null, "edition": null}
            }),
        ))
        .await
        .unwrap();
    assert_eq!(invalid.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let invalid = response_json(invalid).await;
    assert_eq!(invalid["error"]["code"], "invalid_template");
    assert!(invalid["error"]["details"]["template"].as_str().is_some());
}
