use axum::Router;

use crate::jobs::JobRuntime;

/// Builds the stateless HTTP router without opening a listener.
pub fn app() -> Router {
    api_app(crate::api::v1::router())
}

/// Builds the complete router with persistent job APIs enabled.
pub fn job_app(runtime: JobRuntime) -> Router {
    api_app(crate::api::v1::job_router(runtime))
}

fn api_app(v1: Router) -> Router {
    Router::new()
        .nest("/api/v1", v1)
        .fallback(crate::api::error::not_found)
}
