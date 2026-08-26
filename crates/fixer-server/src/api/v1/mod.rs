mod health;
mod jobs;
mod providers;

use axum::{Router, routing::get};

pub(crate) fn router() -> Router {
    Router::new()
        .route(
            "/health",
            get(health::get).fallback(crate::api::error::method_not_allowed),
        )
        .route(
            "/providers",
            get(providers::get).fallback(crate::api::error::method_not_allowed),
        )
        .fallback(crate::api::error::not_found)
}

pub(crate) fn job_router(runtime: crate::jobs::JobRuntime) -> Router {
    router().merge(jobs::router(runtime))
}
