use std::collections::BTreeMap;

use axum::{
    Json,
    http::{HeaderValue, StatusCode},
    response::{IntoResponse, Response},
};
use serde::Serialize;

#[derive(Debug, Serialize)]
struct ErrorEnvelope {
    error: ErrorDto,
}

#[derive(Debug, Serialize)]
struct ErrorDto {
    code: &'static str,
    message: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    details: Option<BTreeMap<String, String>>,
    request_id: String,
}

#[derive(Debug)]
pub(crate) struct ApiError {
    status: StatusCode,
    body: ErrorEnvelope,
}

impl ApiError {
    pub(crate) fn new(
        status: StatusCode,
        code: &'static str,
        message: &'static str,
        details: Option<BTreeMap<String, String>>,
    ) -> Self {
        let request_id = crate::observability::current_request_id();
        Self {
            status,
            body: ErrorEnvelope {
                error: ErrorDto {
                    code,
                    message,
                    details,
                    request_id,
                },
            },
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let request_id = HeaderValue::from_str(&self.body.error.request_id)
            .expect("generated request IDs contain only valid header bytes");
        let mut response = (self.status, Json(self.body)).into_response();
        response.headers_mut().insert("x-request-id", request_id);
        response
    }
}

pub(crate) async fn not_found() -> ApiError {
    ApiError::new(
        StatusCode::NOT_FOUND,
        "not_found",
        "API endpoint not found",
        None,
    )
}

pub(crate) async fn method_not_allowed() -> ApiError {
    ApiError::new(
        StatusCode::METHOD_NOT_ALLOWED,
        "method_not_allowed",
        "HTTP method not allowed",
        None,
    )
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use axum::{body::to_bytes, http::StatusCode, response::IntoResponse};
    use serde_json::json;

    use super::ApiError;

    #[tokio::test]
    async fn field_details_are_serialized_in_the_safe_error_envelope() {
        let details = BTreeMap::from([
            ("title".to_owned(), "must not be empty".to_owned()),
            ("year".to_owned(), "must be a number".to_owned()),
        ]);
        let response = ApiError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "invalid_input",
            "Request fields are invalid",
            Some(details),
        )
        .into_response();

        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let request_id = response.headers()["x-request-id"]
            .to_str()
            .unwrap()
            .to_owned();
        let body = to_bytes(response.into_body(), 4096).await.unwrap();
        let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(
            value,
            json!({
                "error": {
                    "code": "invalid_input",
                    "message": "Request fields are invalid",
                    "details": {
                        "title": "must not be empty",
                        "year": "must be a number"
                    },
                    "request_id": request_id
                }
            })
        );
    }
}
