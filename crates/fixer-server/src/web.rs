use std::path::{Path, PathBuf};

use axum::{
    Router,
    extract::Request,
    http::{HeaderValue, header},
    middleware::{self, Next},
    response::Response,
};
use tower_http::services::{ServeDir, ServeFile};

const IMMUTABLE_CACHE: HeaderValue =
    HeaderValue::from_static("public, max-age=31536000, immutable");
const REVALIDATE_CACHE: HeaderValue = HeaderValue::from_static("no-cache");

/// Adds static assets and client-side route fallback to an API router.
pub fn web_app(api: Router, dist_dir: impl AsRef<Path>) -> Router {
    let dist_dir = dist_dir.as_ref().to_path_buf();
    let index = dist_dir.join("index.html");
    let assets = ServeDir::new(dist_dir.join("assets"));
    let files = ServeDir::new(dist_dir).fallback(ServeFile::new(index));
    let web = Router::new()
        .nest_service("/assets", assets)
        .fallback_service(files)
        .layer(middleware::from_fn(cache_web_response));

    api.merge(web)
}

/// Adds request tracing and request IDs around a complete API and Web router.
pub fn observed_web_app(api: Router, dist_dir: impl AsRef<Path>) -> Router {
    crate::observability::observe(web_app(api, dist_dir))
}

async fn cache_web_response(request: Request, next: Next) -> Response {
    let path = request.uri().path().to_owned();
    let mut response = next.run(request).await;
    let content_type = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    let cache_control = if response.status().is_success()
        && !content_type.starts_with("text/html")
        && is_content_hashed_asset(&path)
    {
        IMMUTABLE_CACHE
    } else {
        REVALIDATE_CACHE
    };
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, cache_control);
    response
}

fn is_content_hashed_asset(path: &str) -> bool {
    if !path.starts_with("/assets/") {
        return false;
    }
    PathBuf::from(path)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .is_some_and(|stem| {
            let bytes = stem.as_bytes();
            bytes.len() >= 9
                && bytes[bytes.len() - 9] == b'-'
                && bytes[bytes.len() - 8..]
                    .iter()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
        })
}
