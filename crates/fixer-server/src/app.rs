use axum::Router;

/// Builds the complete HTTP router without opening a listener.
pub fn app() -> Router {
    Router::new().nest("/api/v1", crate::api::v1::router())
}
