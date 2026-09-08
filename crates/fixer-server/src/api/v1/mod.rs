mod auth;
mod health;
mod ingestion;
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
    router().merge(jobs::router(runtime, None, None))
}

pub(crate) fn workspace_router(state: crate::WorkspaceState) -> Router {
    router().merge(workspace::router(state))
}

pub(crate) fn secure_workspace_router(
    runtime: crate::jobs::JobRuntime,
    auth_state: crate::auth::AuthState,
    workspace_state: crate::WorkspaceState,
    ingestion_runtime: crate::IngestionRuntime,
) -> Router {
    secure_router(
        runtime,
        auth_state,
        Some(workspace_state),
        Some(ingestion_runtime),
    )
}

pub(crate) fn secure_job_router(
    runtime: crate::jobs::JobRuntime,
    auth_state: crate::auth::AuthState,
) -> Router {
    secure_router(runtime, auth_state, None, None)
}

fn secure_router(
    runtime: crate::jobs::JobRuntime,
    auth_state: crate::auth::AuthState,
    workspace_state: Option<crate::WorkspaceState>,
    ingestion_runtime: Option<crate::IngestionRuntime>,
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
        .merge(jobs::router(
            runtime,
            workspace_state.clone(),
            ingestion_runtime.clone(),
        ))
        .merge(auth::protected_router(auth_state.clone()));
    if let Some(workspace_state) = workspace_state {
        protected = protected.merge(workspace::router(workspace_state));
    }
    if let Some(ingestion_runtime) = ingestion_runtime {
        protected = protected.merge(ingestion::router(ingestion_runtime));
    }
    let protected = protected.route_layer(middleware::from_fn_with_state(
        auth_state,
        crate::auth::require_auth,
    ));
    public
        .merge(protected)
        .fallback(crate::api::error::not_found)
}
