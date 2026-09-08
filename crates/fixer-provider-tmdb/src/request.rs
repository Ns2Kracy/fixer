//! Shared authentication for every TMDB endpoint.

use crate::{TmdbConfig, TmdbError};
use fixer_core::{Header, HttpClient, HttpError, HttpMethod, HttpRequest};
use serde::de::DeserializeOwned;

pub async fn get_json<T: DeserializeOwned>(
    config: &TmdbConfig,
    mut url: url::Url,
    http: &dyn HttpClient,
) -> Result<T, TmdbError> {
    let token = config.token();
    let api_key = token.len() == 32 && token.bytes().all(|byte| byte.is_ascii_hexdigit());
    if api_key {
        url.query_pairs_mut().append_pair("api_key", token);
    }
    let mut request = HttpRequest::new(HttpMethod::Get, url.to_string());
    if !api_key {
        request = request.with_header(
            Header::new("authorization", format!("Bearer {token}"))
                .map_err(|error| TmdbError::InvalidData(error.to_string()))?,
        );
    }
    let response = http.execute(request).await.map_err(|error| {
        // Custom transports may include the complete URL or headers in failures.
        let error = match error {
            HttpError::Transport(message) => {
                HttpError::Transport(message.replace(token, "[REDACTED]"))
            }
            HttpError::InvalidMessage(message) => {
                HttpError::InvalidMessage(message.replace(token, "[REDACTED]"))
            }
            other => other,
        };
        TmdbError::from_http(error)
    })?;
    if !(200..300).contains(&response.status) {
        return Err(TmdbError::from_http(HttpError::Status {
            status: response.status,
        }));
    }
    serde_json::from_slice(&response.body).map_err(|error| {
        TmdbError::MalformedResponse(error.to_string().replace(token, "[REDACTED]"))
    })
}
