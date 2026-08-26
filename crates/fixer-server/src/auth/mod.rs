pub mod password;
pub mod session;
pub mod token;

use std::{fmt, time::Duration};

use axum::{
    body::Body,
    extract::{Request, State},
    http::{HeaderMap, HeaderValue, Method, StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Response},
};
use thiserror::Error;
use url::Url;

use crate::{api::error::ApiError, store::SqliteJobStore};

pub use session::IssuedSession;
pub use token::IssuedApiToken;

pub const SESSION_COOKIE_NAME: &str = "fixer_session";
pub const CSRF_HEADER_NAME: &str = "x-csrf-token";
const DEFAULT_SESSION_LIFETIME: Duration = Duration::from_secs(12 * 60 * 60);

/// Authentication and browser-origin policy shared by secure HTTP routes.
#[derive(Clone)]
pub struct AuthState {
    store: SqliteJobStore,
    secure_cookie: bool,
    session_lifetime: Duration,
    allowed_origins: Vec<String>,
}

impl AuthState {
    pub fn new(store: SqliteJobStore) -> Self {
        Self {
            store,
            secure_cookie: false,
            session_lifetime: DEFAULT_SESSION_LIFETIME,
            allowed_origins: Vec::new(),
        }
    }

    pub fn with_secure_cookie(mut self, secure: bool) -> Self {
        self.secure_cookie = secure;
        self
    }

    pub fn with_session_lifetime(mut self, lifetime: Duration) -> Result<Self, AuthConfigError> {
        if lifetime.is_zero() {
            return Err(AuthConfigError::EmptySessionLifetime);
        }
        self.session_lifetime = lifetime;
        Ok(self)
    }

    pub fn with_allowed_origins<I, S>(mut self, origins: I) -> Result<Self, AuthConfigError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut allowed = origins
            .into_iter()
            .map(|origin| validate_origin(origin.as_ref()))
            .collect::<Result<Vec<_>, _>>()?;
        allowed.sort();
        allowed.dedup();
        self.allowed_origins = allowed;
        Ok(self)
    }

    pub(crate) const fn store(&self) -> &SqliteJobStore {
        &self.store
    }

    pub(crate) const fn secure_cookie(&self) -> bool {
        self.secure_cookie
    }

    pub(crate) const fn session_lifetime(&self) -> Duration {
        self.session_lifetime
    }
}

impl fmt::Debug for AuthState {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AuthState")
            .field("secure_cookie", &self.secure_cookie)
            .field("session_lifetime", &self.session_lifetime)
            .field("allowed_origins", &self.allowed_origins)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Error)]
pub enum AuthConfigError {
    #[error("session lifetime must be positive")]
    EmptySessionLifetime,
    #[error(
        "CORS origin `{0}` must be an exact HTTP or HTTPS origin without credentials, path, query, fragment, or wildcard"
    )]
    InvalidOrigin(String),
}

pub(crate) async fn require_auth(
    State(state): State<AuthState>,
    request: Request,
    next: Next,
) -> Response {
    match authenticate_request(&state, request.headers(), request.method()).await {
        Ok(()) => next.run(request).await,
        Err(response) => response,
    }
}

pub(crate) async fn enforce_cors(
    State(state): State<AuthState>,
    request: Request,
    next: Next,
) -> Response {
    let Some(origin) = request.headers().get(header::ORIGIN).cloned() else {
        return next.run(request).await;
    };
    let method = request.method().clone();
    let Ok(origin_text) = origin.to_str() else {
        return forbidden("cors_origin_denied", "Request origin is not allowed");
    };
    if !state
        .allowed_origins
        .iter()
        .any(|allowed| allowed == origin_text)
    {
        return forbidden("cors_origin_denied", "Request origin is not allowed");
    }

    let mut response = if method == Method::OPTIONS {
        Response::new(Body::empty())
    } else {
        next.run(request).await
    };
    if method == Method::OPTIONS {
        *response.status_mut() = StatusCode::NO_CONTENT;
        response.headers_mut().insert(
            header::ACCESS_CONTROL_ALLOW_METHODS,
            HeaderValue::from_static("GET, HEAD, POST, OPTIONS"),
        );
        response.headers_mut().insert(
            header::ACCESS_CONTROL_ALLOW_HEADERS,
            HeaderValue::from_static(
                "authorization, content-type, idempotency-key, last-event-id, x-csrf-token",
            ),
        );
    }
    response
        .headers_mut()
        .insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, origin);
    response.headers_mut().insert(
        header::ACCESS_CONTROL_ALLOW_CREDENTIALS,
        HeaderValue::from_static("true"),
    );
    response
        .headers_mut()
        .append(header::VARY, HeaderValue::from_static("Origin"));
    response
}

async fn authenticate_request(
    state: &AuthState,
    headers: &HeaderMap,
    method: &Method,
) -> Result<(), Response> {
    if let Some(token) = bearer_token(headers) {
        return match state.store.authenticate_api_token(token).await {
            Ok(Some(_)) => Ok(()),
            Ok(None) => Err(unauthorized()),
            Err(_) => Err(authentication_unavailable()),
        };
    }

    let Some(token) = cookie_value(headers, SESSION_COOKIE_NAME) else {
        return Err(unauthorized());
    };
    let session_valid = state
        .store
        .authenticate_session(token, None)
        .await
        .map_err(|_| authentication_unavailable())?;
    if !session_valid {
        return Err(unauthorized());
    }
    if is_state_changing(method) {
        let Some(csrf) = headers
            .get(CSRF_HEADER_NAME)
            .and_then(|value| value.to_str().ok())
        else {
            return Err(forbidden(
                "csrf_validation_failed",
                "CSRF token is missing or invalid",
            ));
        };
        let csrf_valid = state
            .store
            .authenticate_session(token, Some(csrf))
            .await
            .map_err(|_| authentication_unavailable())?;
        if !csrf_valid {
            return Err(forbidden(
                "csrf_validation_failed",
                "CSRF token is missing or invalid",
            ));
        }
    }
    Ok(())
}

pub(crate) fn cookie_value<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(';'))
        .filter_map(|pair| pair.trim().split_once('='))
        .find_map(|(candidate, value)| (candidate == name && !value.is_empty()).then_some(value))
}

pub(crate) fn session_cookie(token: &str, secure: bool, max_age_seconds: u64) -> String {
    let secure = if secure { "; Secure" } else { "" };
    format!(
        "{SESSION_COOKIE_NAME}={token}; Path=/api; HttpOnly; SameSite=Strict; Max-Age={max_age_seconds}{secure}"
    )
}

pub(crate) fn expired_session_cookie(secure: bool) -> String {
    session_cookie("", secure, 0)
}

fn bearer_token(headers: &HeaderMap) -> Option<&str> {
    let value = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    let token = value.strip_prefix("Bearer ")?;
    (!token.is_empty() && !token.bytes().any(|byte| byte.is_ascii_whitespace())).then_some(token)
}

fn is_state_changing(method: &Method) -> bool {
    matches!(
        *method,
        Method::POST | Method::PUT | Method::PATCH | Method::DELETE
    )
}

fn validate_origin(origin: &str) -> Result<String, AuthConfigError> {
    let parsed =
        Url::parse(origin).map_err(|_| AuthConfigError::InvalidOrigin(origin.to_owned()))?;
    if !matches!(parsed.scheme(), "http" | "https")
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.host_str().is_none()
        || parsed.path() != "/"
        || parsed.query().is_some()
        || parsed.fragment().is_some()
        || origin.contains('*')
    {
        return Err(AuthConfigError::InvalidOrigin(origin.to_owned()));
    }
    Ok(origin.trim_end_matches('/').to_owned())
}

fn unauthorized() -> Response {
    ApiError::new(
        StatusCode::UNAUTHORIZED,
        "authentication_required",
        "Valid authentication is required",
        None,
    )
    .into_response()
}

fn forbidden(code: &'static str, message: &'static str) -> Response {
    ApiError::new(StatusCode::FORBIDDEN, code, message, None).into_response()
}

fn authentication_unavailable() -> Response {
    ApiError::new(
        StatusCode::INTERNAL_SERVER_ERROR,
        "authentication_unavailable",
        "Authentication service is unavailable",
        None,
    )
    .into_response()
}
