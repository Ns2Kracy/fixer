mod auth;
mod health;
mod jobs;
mod providers;
mod workspace;

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

pub(crate) fn workspace_router(state: crate::WorkspaceState) -> Router {
    router().merge(workspace::router(state))
}

pub(crate) fn secure_workspace_router(
    runtime: crate::jobs::JobRuntime,
    auth_state: crate::auth::AuthState,
    workspace_state: crate::WorkspaceState,
) -> Router {
    secure_router(runtime, auth_state, Some(workspace_state))
}

pub(crate) fn secure_job_router(
    runtime: crate::jobs::JobRuntime,
    auth_state: crate::auth::AuthState,
) -> Router {
    secure_router(runtime, auth_state, None)
}

fn secure_router(
    runtime: crate::jobs::JobRuntime,
    auth_state: crate::auth::AuthState,
    workspace_state: Option<crate::WorkspaceState>,
) -> Router {
    let public = Router::new()
        .route(
            "/health",
            get(health::get).fallback(crate::api::error::method_not_allowed),
        )
        .merge(auth::public_router(auth_state.clone()));
    let mut protected = Router::new()
        .route(
            "/providers",
            get(providers::get).fallback(crate::api::error::method_not_allowed),
        )
        .merge(jobs::router(runtime))
        .merge(auth::protected_router(auth_state.clone()));
    if let Some(workspace_state) = workspace_state {
        protected = protected.merge(workspace::router(workspace_state));
    }
    let protected = protected.route_layer(middleware::from_fn_with_state(
        auth_state,
        crate::auth::require_auth,
    ));
    public
        .merge(protected)
        .fallback(crate::api::error::not_found)
}
