mod auth;
mod health;
mod jobs;
mod providers;

use axum::{Router, middleware, routing::get};

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

pub(crate) fn secure_job_router(
    runtime: crate::jobs::JobRuntime,
    auth_state: crate::auth::AuthState,
) -> Router {
    let public = Router::new()
        .route(
            "/health",
            get(health::get).fallback(crate::api::error::method_not_allowed),
        )
        .merge(auth::public_router(auth_state.clone()));
    let protected = Router::new()
        .route(
            "/providers",
            get(providers::get).fallback(crate::api::error::method_not_allowed),
        )
        .merge(jobs::router(runtime))
        .merge(auth::protected_router(auth_state.clone()))
        .route_layer(middleware::from_fn_with_state(
            auth_state,
            crate::auth::require_auth,
        ));
    public
        .merge(protected)
        .fallback(crate::api::error::not_found)
}
