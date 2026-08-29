use axum::Router;

use crate::{WorkspaceState, auth::AuthState, jobs::JobRuntime};

/// Builds the stateless HTTP router without opening a listener.
pub fn app() -> Router {
    api_app(crate::api::v1::router())
}

/// Builds the complete router with persistent job APIs enabled.
pub fn job_app(runtime: JobRuntime) -> Router {
    api_app(crate::api::v1::job_router(runtime))
}

/// Builds the workspace router for settings, library, search, providers, and templates.
pub fn workspace_app(state: WorkspaceState) -> Router {
    api_app(crate::api::v1::workspace_router(state))
}

/// Builds the complete router with authentication, CSRF, and CORS enforced.
pub fn secure_job_app(runtime: JobRuntime, auth_state: AuthState) -> Router {
    api_app(crate::api::v1::secure_job_router(
        runtime,
        auth_state.clone(),
    ))
    .layer(axum::middleware::from_fn_with_state(
        auth_state.clone(),
        crate::auth::resolve_client_ip,
    ))
    .layer(axum::middleware::from_fn_with_state(
        auth_state,
        crate::auth::enforce_cors,
    ))
}

/// Builds the authenticated production router with jobs and workspace APIs enabled.
pub fn secure_workspace_app(
    runtime: JobRuntime,
    auth_state: AuthState,
    workspace_state: WorkspaceState,
) -> Router {
    api_app(crate::api::v1::secure_workspace_router(
        runtime,
        auth_state.clone(),
        workspace_state,
    ))
    .layer(axum::middleware::from_fn_with_state(
        auth_state.clone(),
        crate::auth::resolve_client_ip,
    ))
    .layer(axum::middleware::from_fn_with_state(
        auth_state,
        crate::auth::enforce_cors,
    ))
}

fn api_app(v1: Router) -> Router {
    let api = Router::new()
        .route("/", axum::routing::any(crate::api::error::not_found))
        .nest("/v1", v1)
        .fallback(crate::api::error::not_found);
    Router::new()
        .route("/api/", axum::routing::any(crate::api::error::not_found))
        .nest("/api", api)
}
