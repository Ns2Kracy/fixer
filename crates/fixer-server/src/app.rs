use axum::Router;

use crate::{auth::AuthState, jobs::JobRuntime};

/// Builds the stateless HTTP router without opening a listener.
pub fn app() -> Router {
    api_app(crate::api::v1::router())
}

/// Builds the complete router with persistent job APIs enabled.
pub fn job_app(runtime: JobRuntime) -> Router {
    api_app(crate::api::v1::job_router(runtime))
}

/// Builds the complete router with authentication, CSRF, and CORS enforced.
pub fn secure_job_app(runtime: JobRuntime, auth_state: AuthState) -> Router {
    api_app(crate::api::v1::secure_job_router(
        runtime,
        auth_state.clone(),
    ))
    .layer(axum::middleware::from_fn_with_state(
        auth_state,
        crate::auth::enforce_cors,
    ))
}

fn api_app(v1: Router) -> Router {
    Router::new()
        .nest("/api/v1", v1)
        .fallback(crate::api::error::not_found)
}
