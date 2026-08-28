use std::fs;

use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use fixer_server::{app, web_app};
use http_body_util::BodyExt;
use serde_json::Value;
use tempfile::TempDir;
use tower::ServiceExt;

const INDEX_HTML: &str = "<!doctype html><html><body><div id=\"root\">Fixer</div></body></html>";

fn fixture_dist() -> TempDir {
    let directory = tempfile::tempdir().unwrap();
    fs::create_dir(directory.path().join("assets")).unwrap();
    fs::write(directory.path().join("index.html"), INDEX_HTML).unwrap();
    fs::write(
        directory.path().join("assets/index-1a2B3c4-.js"),
        "console.log('fixer');",
    )
    .unwrap();
    fs::write(
        directory.path().join("assets/index-1a2B3c4_.css"),
        "body { color: #222; }",
    )
    .unwrap();
    fs::write(
        directory.path().join("assets/module-production.js"),
        "console.log('unhashed');",
    )
    .unwrap();
    directory
}

async fn response_text(response: axum::response::Response) -> String {
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    String::from_utf8(bytes.to_vec()).unwrap()
}

async fn response_json(response: axum::response::Response) -> Value {
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

#[tokio::test]
async fn root_and_client_routes_serve_uncached_html() {
    let dist = fixture_dist();
    let application = web_app(app(), dist.path());

    for path in ["/", "/jobs/42/review", "/settings"] {
        let response = application
            .clone()
            .oneshot(Request::get(path).body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK, "path: {path}");
        assert_eq!(
            response.headers().get(header::CACHE_CONTROL).unwrap(),
            "no-cache"
        );
        assert!(
            response
                .headers()
                .get(header::CONTENT_TYPE)
                .unwrap()
                .to_str()
                .unwrap()
                .starts_with("text/html")
        );
        assert_eq!(response_text(response).await, INDEX_HTML);
    }
}

#[tokio::test]
async fn content_hashed_assets_are_immutable() {
    let dist = fixture_dist();
    let application = web_app(app(), dist.path());

    for (path, body) in [
        ("/assets/index-1a2B3c4-.js", "console.log('fixer');"),
        ("/assets/index-1a2B3c4_.css", "body { color: #222; }"),
    ] {
        let response = application
            .clone()
            .oneshot(Request::get(path).body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK, "path: {path}");
        assert_eq!(
            response.headers().get(header::CACHE_CONTROL).unwrap(),
            "public, max-age=31536000, immutable"
        );
        assert_eq!(response_text(response).await, body);
    }
}

#[tokio::test]
async fn successful_unhashed_assets_require_revalidation() {
    let dist = fixture_dist();
    let response = web_app(app(), dist.path())
        .oneshot(
            Request::get("/assets/module-production.js")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get(header::CACHE_CONTROL).unwrap(),
        "no-cache"
    );
}

#[tokio::test]
async fn missing_assets_do_not_fall_back_to_spa_html() {
    let dist = fixture_dist();
    let application = web_app(app(), dist.path());

    for path in [
        "/assets/missing-script.js",
        "/assets/../index.html",
        "/assets/%2e%2e/index.html",
    ] {
        let response = application
            .clone()
            .oneshot(Request::get(path).body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND, "path: {path}");
        assert_eq!(
            response.headers().get(header::CACHE_CONTROL).unwrap(),
            "no-cache"
        );
        assert_ne!(response_text(response).await, INDEX_HTML);
    }
}

#[tokio::test]
#[ignore = "requires pnpm --dir web build"]
async fn built_vite_output_serves_spa_asset_and_api_probes() {
    let dist = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../web/dist");
    assert!(
        dist.join("index.html").is_file(),
        "run pnpm --dir web build"
    );
    let asset_name = fs::read_dir(dist.join("assets"))
        .unwrap()
        .filter_map(Result::ok)
        .map(|entry| entry.file_name())
        .find(|name| {
            let name = name.to_string_lossy();
            name.starts_with("index-") && name.ends_with(".js")
        })
        .expect("Vite build must emit a hashed index JavaScript asset");
    let asset_path = format!("/assets/{}", asset_name.to_string_lossy());
    let application = web_app(app(), &dist);

    for path in ["/", "/settings"] {
        let response = application
            .clone()
            .oneshot(Request::get(path).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK, "path: {path}");
        assert_eq!(
            response.headers().get(header::CACHE_CONTROL).unwrap(),
            "no-cache"
        );
    }

    let asset = application
        .clone()
        .oneshot(Request::get(asset_path).body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(asset.status(), StatusCode::OK);
    assert_eq!(
        asset.headers().get(header::CACHE_CONTROL).unwrap(),
        "public, max-age=31536000, immutable"
    );

    let health = application
        .oneshot(Request::get("/api/v1/health").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(health.status(), StatusCode::OK);
    assert_eq!(response_json(health).await["status"], "ok");
}

#[tokio::test]
async fn api_routes_and_api_404s_never_fall_back_to_html() {
    let dist = fixture_dist();
    let application = web_app(app(), dist.path());

    let health = application
        .clone()
        .oneshot(Request::get("/api/v1/health").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(health.status(), StatusCode::OK);
    assert_eq!(response_json(health).await["status"], "ok");

    for path in ["/api", "/api/", "/api/v1/missing", "/api/v2/health"] {
        let response = application
            .clone()
            .oneshot(Request::get(path).body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND, "path: {path}");
        assert!(
            response.headers().get(header::CACHE_CONTROL).is_none(),
            "API responses must not inherit Web cache policy: {path}"
        );
        let body = response_json(response).await;
        assert_eq!(body["error"]["code"], "not_found");
        assert_eq!(body["error"]["message"], "API endpoint not found");
    }
}
