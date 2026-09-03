use std::collections::BTreeMap;

use axum::{
    Json, Router,
    extract::{State, rejection::JsonRejection},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde::{Deserialize, Serialize};

use crate::{
    api::error::ApiError,
    auth::{AuthState, SESSION_COOKIE_NAME, cookie_value, expired_session_cookie, session_cookie},
};

const SCHEMA_VERSION: u8 = 1;
const MIN_USERNAME_CHARS: usize = 3;
const MAX_USERNAME_CHARS: usize = 64;
const MIN_REGISTRATION_PASSWORD_BYTES: usize = 8;
const MAX_PASSWORD_BYTES: usize = 1024;

pub fn public_router(state: AuthState) -> Router {
    Router::new()
        .route("/auth/status", get(status))
        .route("/auth/register", post(register))
        .route("/auth/login", post(login))
        .with_state(state)
}

pub fn protected_router(state: AuthState) -> Router {
    Router::new()
        .route("/auth/logout", post(logout))
        .with_state(state)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CredentialsRequest {
    username: String,
    password: String,
}

#[derive(Serialize)]
struct AuthStatusResponse {
    schema_version: u8,
    registration_required: bool,
    authenticated: bool,
    username: Option<String>,
}

#[derive(Serialize)]
struct SessionResponse {
    schema_version: u8,
    username: String,
    csrf_token: String,
    expires_at_ms: i64,
}

async fn status(State(state): State<AuthState>, headers: HeaderMap) -> Result<Response, ApiError> {
    let username = state
        .store()
        .registered_username()
        .await
        .map_err(|_| unavailable())?;
    let registration_required = username.is_none();
    let authenticated = if registration_required {
        false
    } else if let Some(token) = cookie_value(&headers, SESSION_COOKIE_NAME) {
        state
            .store()
            .authenticate_session(token, None)
            .await
            .map_err(|_| unavailable())?
    } else {
        false
    };
    Ok(no_store_json(AuthStatusResponse {
        schema_version: SCHEMA_VERSION,
        registration_required,
        authenticated,
        username: if authenticated { username } else { None },
    }))
}

async fn register(
    State(state): State<AuthState>,
    request: Result<Json<CredentialsRequest>, JsonRejection>,
) -> Result<Response, ApiError> {
    let Json(request) = request.map_err(|_| invalid_registration(None))?;
    let username = request.username.trim().to_owned();
    validate_registration(&username, &request.password)?;

    if state
        .store()
        .has_registered_user()
        .await
        .map_err(|_| unavailable())?
    {
        return Err(registration_closed());
    }

    let password = request.password;
    let password_hash =
        tokio::task::spawn_blocking(move || crate::auth::password::hash_password(&password))
            .await
            .map_err(|_| unavailable())?
            .map_err(|_| unavailable())?;
    let registered = state
        .store()
        .register_single_user(&username, &password_hash)
        .await
        .map_err(|_| unavailable())?;
    if !registered {
        return Err(registration_closed());
    }
    session_response(&state, username).await
}

async fn login(
    State(state): State<AuthState>,
    request: Result<Json<CredentialsRequest>, JsonRejection>,
) -> Result<Response, ApiError> {
    let Json(request) = request.map_err(|_| invalid_login())?;
    let username = request.username.trim();
    if username.is_empty()
        || username.chars().count() > MAX_USERNAME_CHARS
        || request.password.is_empty()
        || request.password.len() > MAX_PASSWORD_BYTES
    {
        return Err(invalid_login());
    }
    let verified = state
        .store()
        .verify_single_user_credentials(username, &request.password)
        .await
        .map_err(|_| unavailable())?;
    if !verified {
        return Err(invalid_login());
    }
    session_response(&state, username.to_owned()).await
}

async fn session_response(state: &AuthState, username: String) -> Result<Response, ApiError> {
    let session = state
        .store()
        .create_session(state.session_lifetime())
        .await
        .map_err(|_| unavailable())?;
    let cookie = HeaderValue::from_str(&session_cookie(
        session.token(),
        state.secure_cookie(),
        state.session_lifetime().as_secs(),
    ))
    .map_err(|_| unavailable())?;
    let mut response = Json(SessionResponse {
        schema_version: SCHEMA_VERSION,
        username,
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

fn validate_registration(username: &str, password: &str) -> Result<(), ApiError> {
    let mut details = BTreeMap::new();
    if !(MIN_USERNAME_CHARS..=MAX_USERNAME_CHARS).contains(&username.chars().count()) {
        details.insert(
            "username".to_owned(),
            format!(
                "must contain between {MIN_USERNAME_CHARS} and {MAX_USERNAME_CHARS} characters"
            ),
        );
    }
    if !(MIN_REGISTRATION_PASSWORD_BYTES..=MAX_PASSWORD_BYTES).contains(&password.len()) {
        details.insert(
            "password".to_owned(),
            format!(
                "must contain between {MIN_REGISTRATION_PASSWORD_BYTES} and {MAX_PASSWORD_BYTES} bytes"
            ),
        );
    }
    if details.is_empty() {
        Ok(())
    } else {
        Err(invalid_registration(Some(details)))
    }
}

fn no_store_json<T: Serialize>(body: T) -> Response {
    let mut response = Json(body).into_response();
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

fn invalid_registration(details: Option<BTreeMap<String, String>>) -> ApiError {
    ApiError::new(
        StatusCode::UNPROCESSABLE_ENTITY,
        "invalid_registration",
        "Registration fields are invalid",
        details,
    )
}

fn registration_closed() -> ApiError {
    ApiError::new(
        StatusCode::CONFLICT,
        "registration_closed",
        "Administrator registration is closed",
        None,
    )
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
