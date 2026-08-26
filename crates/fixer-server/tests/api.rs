use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use fixer_server::app;
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tower::ServiceExt;

async fn response_json(response: axum::response::Response) -> Value {
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

#[tokio::test]
async fn health_returns_a_stable_versioned_dto() {
    let response = app()
        .oneshot(Request::get("/api/v1/health").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response_json(response).await,
        json!({
            "schema_version": 1,
            "status": "ok",
            "version": env!("CARGO_PKG_VERSION"),
        })
    );
}

#[tokio::test]
async fn providers_return_application_owned_capability_dtos() {
    let response = app()
        .oneshot(
            Request::get("/api/v1/providers")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response_json(response).await,
        json!({
            "schema_version": 1,
            "providers": [
                {"id": "local", "name": "Local metadata", "media_kinds": ["movie", "television", "anime", "music", "book"], "network": false, "optional": false},
                {"id": "tmdb", "name": "The Movie Database", "media_kinds": ["movie", "television"], "network": true, "optional": false},
                {"id": "bangumi", "name": "Bangumi", "media_kinds": ["anime"], "network": true, "optional": false},
                {"id": "anilist", "name": "AniList", "media_kinds": ["anime"], "network": true, "optional": true},
                {"id": "musicbrainz", "name": "MusicBrainz", "media_kinds": ["music"], "network": true, "optional": false},
                {"id": "openlibrary", "name": "Open Library", "media_kinds": ["book"], "network": true, "optional": false}
            ]
        })
    );
}

#[tokio::test]
async fn method_errors_use_the_same_safe_envelope() {
    let response = app()
        .oneshot(Request::post("/api/v1/health").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
    let body = response_json(response).await;
    assert_eq!(body["error"]["code"], "method_not_allowed");
    assert_eq!(body["error"]["message"], "HTTP method not allowed");
    assert!(
        body["error"]["request_id"]
            .as_str()
            .is_some_and(|id| !id.is_empty())
    );
}

#[tokio::test]
async fn unsupported_api_versions_use_the_safe_error_envelope() {
    let response = app()
        .oneshot(Request::get("/api/v2/health").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert!(response.headers().contains_key("x-request-id"));
    let body = response_json(response).await;
    assert_eq!(body["error"]["code"], "not_found");
    assert_eq!(body["error"]["message"], "API endpoint not found");
}

#[tokio::test]
async fn api_errors_are_safe_versioned_envelopes_with_request_ids() {
    let response = app()
        .oneshot(Request::get("/api/v1/missing").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let header_request_id = response
        .headers()
        .get("x-request-id")
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();
    let body = response_json(response).await;
    assert_eq!(body["error"]["code"], "not_found");
    assert_eq!(body["error"]["message"], "API endpoint not found");
    assert!(body["error"].get("details").is_none());
    assert_eq!(body["error"]["request_id"], header_request_id);
    assert!(!header_request_id.is_empty());
}
