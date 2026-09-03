//! Reqwest implementation of the core HTTP protocol.

use crate::{HttpConfig, HttpConfigError};
use fixer_core::{BoxFuture, Header, HttpClient, HttpError, HttpMethod, HttpRequest, HttpResponse};

/// Rustls-backed default HTTP transport.
#[derive(Clone)]
pub struct ReqwestHttpClient {
    client: reqwest::Client,
}

impl ReqwestHttpClient {
    /// Constructs a client. Reqwest's system-proxy support remains enabled unless an explicit proxy is supplied.
    pub fn new(config: HttpConfig) -> Result<Self, HttpConfigError> {
        config.validate()?;
        let mut builder = reqwest::Client::builder()
            .use_rustls_tls()
            .timeout(config.timeout)
            .user_agent(config.user_agent);
        if let Some(proxy) = config.proxy {
            builder = builder.proxy(
                reqwest::Proxy::all(proxy.as_str()).map_err(|_| HttpConfigError::InvalidProxy)?,
            );
        }
        let client = builder
            .build()
            .map_err(|error| HttpConfigError::Client(error.to_string()))?;
        Ok(Self { client })
    }
}

impl HttpClient for ReqwestHttpClient {
    fn execute(&self, request: HttpRequest) -> BoxFuture<'_, Result<HttpResponse, HttpError>> {
        Box::pin(async move {
            let method = match request.method {
                HttpMethod::Get => reqwest::Method::GET,
                HttpMethod::Post => reqwest::Method::POST,
                HttpMethod::Put => reqwest::Method::PUT,
                HttpMethod::Patch => reqwest::Method::PATCH,
                HttpMethod::Delete => reqwest::Method::DELETE,
                HttpMethod::Head => reqwest::Method::HEAD,
            };
            let mut outgoing = self.client.request(method, &request.url);
            for header in request.headers {
                let name = reqwest::header::HeaderName::from_bytes(header.name().as_bytes())
                    .map_err(|error| HttpError::InvalidMessage(error.to_string()))?;
                let value = reqwest::header::HeaderValue::from_str(header.value())
                    .map_err(|error| HttpError::InvalidMessage(error.to_string()))?;
                outgoing = outgoing.header(name, value);
            }
            if !request.body.is_empty() {
                outgoing = outgoing.body(request.body);
            }
            let response = outgoing
                .send()
                .await
                .map_err(|error| map_reqwest_error(&error))?;
            let status = response.status().as_u16();
            if !(200..300).contains(&status) {
                return Err(HttpError::Status { status });
            }
            let headers = response
                .headers()
                .iter()
                .filter_map(|(name, value)| Header::new(name.as_str(), value.to_str().ok()?).ok())
                .collect();
            let body = response
                .bytes()
                .await
                .map_err(|error| map_reqwest_error(&error))?
                .to_vec();
            Ok(HttpResponse {
                status,
                headers,
                body,
            })
        })
    }
}

fn map_reqwest_error(error: &reqwest::Error) -> HttpError {
    if error.is_timeout() {
        HttpError::Timeout
    } else if error.is_builder() {
        HttpError::InvalidMessage(error.to_string())
    } else {
        HttpError::Transport(error.to_string())
    }
}
