use std::{
    fmt::Write as _,
    sync::atomic::{AtomicU64, Ordering},
};

use axum::http::{HeaderValue, Request as HttpRequest, header::HeaderName};
use axum::{
    Router,
    body::Body,
    extract::Request,
    middleware::{self, Next},
    response::Response,
};
use fixer_runtime::{LoggingConfig, LoggingFormat};
use thiserror::Error;
use tower::ServiceBuilder;
use tower_http::{
    request_id::{MakeRequestId, PropagateRequestIdLayer, RequestId, SetRequestIdLayer},
    trace::{DefaultOnResponse, TraceLayer},
};
use tracing::Level;
use tracing_subscriber::{EnvFilter, layer::SubscriberExt as _, util::SubscriberInitExt as _};

const REQUEST_ID_HEADER: HeaderName = HeaderName::from_static("x-request-id");
static FALLBACK_SEQUENCE: AtomicU64 = AtomicU64::new(1);

tokio::task_local! {
    static CURRENT_REQUEST_ID: String;
}

#[derive(Debug, Error)]
pub enum TracingInitError {
    #[error("invalid tracing filter: {0}")]
    Filter(String),
    #[error("failed to install tracing subscriber: {0}")]
    Install(String),
}

pub fn init_tracing(config: &LoggingConfig) -> Result<(), TracingInitError> {
    let filter = EnvFilter::try_new(&config.filter)
        .map_err(|error| TracingInitError::Filter(error.to_string()))?;
    let result = match config.format {
        LoggingFormat::Pretty => tracing_subscriber::registry()
            .with(filter)
            .with(tracing_subscriber::fmt::layer())
            .try_init(),
        LoggingFormat::Json => tracing_subscriber::registry()
            .with(filter)
            .with(tracing_subscriber::fmt::layer().json())
            .try_init(),
    };
    result.map_err(|error| TracingInitError::Install(error.to_string()))
}

#[derive(Debug, Clone, Copy, Default)]
struct MakeFixerRequestId;

impl MakeRequestId for MakeFixerRequestId {
    fn make_request_id<B>(&mut self, _request: &HttpRequest<B>) -> Option<RequestId> {
        HeaderValue::from_str(&fresh_request_id())
            .ok()
            .map(RequestId::new)
    }
}

pub(crate) fn observe(router: Router) -> Router {
    router.layer(
        ServiceBuilder::new()
            .layer(middleware::from_fn(normalize_request_id))
            .layer(SetRequestIdLayer::new(
                REQUEST_ID_HEADER,
                MakeFixerRequestId,
            ))
            .layer(
                TraceLayer::new_for_http()
                    .make_span_with(|request: &Request<Body>| {
                        let request_id = request
                            .extensions()
                            .get::<RequestId>()
                            .and_then(|value| value.header_value().to_str().ok())
                            .unwrap_or("missing");
                        tracing::info_span!(
                            "http.request",
                            request_id,
                            method = %request.method(),
                            path = %request.uri().path(),
                            version = ?request.version(),
                        )
                    })
                    .on_response(DefaultOnResponse::new().level(Level::INFO)),
            )
            .layer(PropagateRequestIdLayer::new(REQUEST_ID_HEADER))
            .layer(middleware::from_fn(bind_request_id)),
    )
}

pub(crate) fn current_request_id() -> String {
    CURRENT_REQUEST_ID
        .try_with(Clone::clone)
        .unwrap_or_else(|_| fresh_request_id())
}

async fn normalize_request_id(mut request: Request, next: Next) -> Response {
    let should_remove = request
        .headers()
        .get(&REQUEST_ID_HEADER)
        .is_some_and(|value| !valid_request_id(value));
    if should_remove {
        request.headers_mut().remove(&REQUEST_ID_HEADER);
    }
    next.run(request).await
}

async fn bind_request_id(request: Request, next: Next) -> Response {
    let request_id = request
        .extensions()
        .get::<RequestId>()
        .and_then(|value| value.header_value().to_str().ok())
        .map(str::to_owned)
        .unwrap_or_else(fresh_request_id);
    CURRENT_REQUEST_ID
        .scope(request_id, next.run(request))
        .await
}

fn valid_request_id(value: &HeaderValue) -> bool {
    value.to_str().is_ok_and(|value| {
        !value.is_empty()
            && value.len() <= 128
            && value.bytes().all(|byte| {
                byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':')
            })
    })
}

fn fresh_request_id() -> String {
    let mut random = [0_u8; 16];
    if getrandom::fill(&mut random).is_err() {
        let sequence = FALLBACK_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        return fallback_request_id(sequence);
    }
    let mut request_id = String::with_capacity(36);
    request_id.push_str("req-");
    for byte in random {
        write!(request_id, "{byte:02x}").expect("writing to a String cannot fail");
    }
    request_id
}

fn fallback_request_id(sequence: u64) -> String {
    format!("req-{sequence:032x}")
}

#[cfg(test)]
mod tests {
    use super::fallback_request_id;

    #[test]
    fn fallback_request_ids_keep_the_normal_generated_shape() {
        let request_id = fallback_request_id(1);

        assert_eq!(request_id, "req-00000000000000000000000000000001");
        assert_eq!(request_id.len(), 36);
    }
}
