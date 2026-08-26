use axum::{
    Json, Router,
    extract::{State, rejection::JsonRejection},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::post,
};
use serde::{Deserialize, Serialize};

use crate::{
    api::error::ApiError,
    auth::{AuthState, SESSION_COOKIE_NAME, cookie_value, expired_session_cookie, session_cookie},
};

const SCHEMA_VERSION: u8 = 1;

pub(crate) fn public_router(state: AuthState) -> Router {
    Router::new()
        .route("/auth/login", post(login))
        .with_state(state)
}

pub(crate) fn protected_router(state: AuthState) -> Router {
    Router::new()
        .route("/auth/logout", post(logout))
        .with_state(state)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LoginRequest {
    password: String,
}

#[derive(Serialize)]
struct LoginResponse {
    schema_version: u8,
    csrf_token: String,
    expires_at_ms: i64,
}

async fn login(
    State(state): State<AuthState>,
    request: Result<Json<LoginRequest>, JsonRejection>,
) -> Result<Response, ApiError> {
    let Json(request) = request.map_err(|_| invalid_login())?;
    if request.password.is_empty() || request.password.len() > 1024 {
        return Err(invalid_login());
    }
    let verified = state
        .store()
        .verify_single_user_password(&request.password)
        .await
        .map_err(|_| unavailable())?;
    if !verified {
        return Err(invalid_login());
    }
    let session = state
        .store()
        .create_session(state.session_lifetime())
        .await
        .map_err(|_| unavailable())?;
    let max_age = state.session_lifetime().as_secs();
    let cookie = HeaderValue::from_str(&session_cookie(
        session.token(),
        state.secure_cookie(),
        max_age,
    ))
    .map_err(|_| unavailable())?;
    let mut response = Json(LoginResponse {
        schema_version: SCHEMA_VERSION,
        csrf_token: session.csrf_token().to_owned(),
        expires_at_ms: session.expires_at_ms(),
    })
    .into_response();
    response.headers_mut().insert(header::SET_COOKIE, cookie);
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    Ok(response)
}

async fn logout(State(state): State<AuthState>, headers: HeaderMap) -> Result<Response, ApiError> {
    if let Some(token) = cookie_value(&headers, SESSION_COOKIE_NAME) {
        state
            .store()
            .revoke_session(token)
            .await
            .map_err(|_| unavailable())?;
    }
    let cookie = HeaderValue::from_str(&expired_session_cookie(state.secure_cookie()))
        .map_err(|_| unavailable())?;
    let mut response = StatusCode::NO_CONTENT.into_response();
    response.headers_mut().insert(header::SET_COOKIE, cookie);
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    Ok(response)
}

fn invalid_login() -> ApiError {
    ApiError::new(
        StatusCode::UNAUTHORIZED,
        "invalid_credentials",
        "Credentials are invalid",
        None,
    )
}

fn unavailable() -> ApiError {
    ApiError::new(
        StatusCode::INTERNAL_SERVER_ERROR,
        "authentication_unavailable",
        "Authentication service is unavailable",
        None,
    )
}
