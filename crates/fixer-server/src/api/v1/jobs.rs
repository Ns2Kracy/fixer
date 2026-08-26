use std::{collections::BTreeMap, num::NonZeroI64};

use axum::{
    Json, Router,
    extract::{
        Path, State,
        rejection::{JsonRejection, PathRejection},
    },
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response, Sse},
    routing::{get, post},
};
use serde::{Deserialize, Serialize};

use crate::{
    api::error::ApiError,
    jobs::{
        JobRuntime, RuntimeError,
        model::{
            ExecutionSummary, JobInputDto, JobMediaKind, JobState, PlanSummary, ProgressSummary,
            ReviewSummary,
        },
    },
    store::{JobId, JobRecord, StoreError},
};

const SCHEMA_VERSION: u8 = 1;

pub(crate) fn router(runtime: JobRuntime) -> Router {
    Router::new()
        .route("/jobs", post(create).fallback(post_only))
        .route("/jobs/{id}", get(get_job).fallback(get_only))
        .route("/jobs/{id}/cancel", post(cancel).fallback(post_only))
        .route("/jobs/{id}/events", get(events).fallback(get_only))
        .with_state(runtime)
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CreateJobRequest {
    media_kind: JobMediaKind,
    input_path: String,
    apply: bool,
}

#[derive(Debug, Serialize)]
struct JobEnvelope {
    schema_version: u8,
    job: JobDto,
}

#[derive(Debug, Serialize)]
struct JobDto {
    id: i64,
    input: JobInputDto,
    state: JobState,
    #[serde(skip_serializing_if = "Option::is_none")]
    progress: Option<ProgressSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    review: Option<ReviewSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    plan: Option<PlanSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    execution: Option<ExecutionSummary>,
    created_at_ms: i64,
    updated_at_ms: i64,
}

async fn create(
    State(runtime): State<JobRuntime>,
    request: Result<Json<CreateJobRequest>, JsonRejection>,
) -> Result<impl IntoResponse, ApiError> {
    let Json(request) = request.map_err(map_json_rejection)?;
    if request.input_path.trim().is_empty() {
        return Err(invalid_input("input_path", "must not be empty"));
    }
    let input = JobInputDto::new(request.media_kind, request.input_path, request.apply);
    let job = runtime.create(input).await.map_err(map_runtime_error)?;
    Ok((StatusCode::ACCEPTED, Json(envelope(job))))
}

async fn get_job(
    State(runtime): State<JobRuntime>,
    path: Result<Path<i64>, PathRejection>,
) -> Result<Json<JobEnvelope>, ApiError> {
    let id = extract_id(path)?;
    let job = runtime.get(id).await.map_err(map_runtime_error)?;
    Ok(Json(envelope(job)))
}

async fn cancel(
    State(runtime): State<JobRuntime>,
    path: Result<Path<i64>, PathRejection>,
) -> Result<Json<JobEnvelope>, ApiError> {
    let id = extract_id(path)?;
    let job = runtime.cancel(id).await.map_err(map_runtime_error)?;
    Ok(Json(envelope(job)))
}

async fn events(
    State(runtime): State<JobRuntime>,
    path: Result<Path<i64>, PathRejection>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, ApiError> {
    let id = extract_id(path)?;
    let cursor = event_cursor(&headers)?;
    let stream = runtime
        .event_stream(id, cursor)
        .await
        .map_err(map_runtime_error)?;
    Ok(Sse::new(stream))
}

async fn post_only() -> Response {
    method_not_allowed("POST")
}

async fn get_only() -> Response {
    method_not_allowed("GET, HEAD")
}

fn method_not_allowed(allow: &'static str) -> Response {
    let mut response = ApiError::new(
        StatusCode::METHOD_NOT_ALLOWED,
        "method_not_allowed",
        "HTTP method not allowed",
        None,
    )
    .into_response();
    response
        .headers_mut()
        .insert(header::ALLOW, HeaderValue::from_static(allow));
    response
}

fn extract_id(path: Result<Path<i64>, PathRejection>) -> Result<JobId, ApiError> {
    let Path(value) = path.map_err(|_| invalid_input("job_id", "must be a positive integer"))?;
    parse_id(value)
}

fn parse_id(value: i64) -> Result<JobId, ApiError> {
    NonZeroI64::new(value)
        .filter(|value| value.get() > 0)
        .map(|_| JobId::from_database(value))
        .transpose()
        .map_err(map_store_error)?
        .ok_or_else(|| not_found(value))
}

fn event_cursor(headers: &HeaderMap) -> Result<Option<&str>, ApiError> {
    headers
        .get("last-event-id")
        .map(|value| {
            value
                .to_str()
                .map_err(|_| invalid_input("last-event-id", "must be a valid event cursor"))
        })
        .transpose()
}

fn envelope(job: JobRecord) -> JobEnvelope {
    JobEnvelope {
        schema_version: SCHEMA_VERSION,
        job: JobDto {
            id: job.id().get(),
            input: job.input().clone(),
            state: job.state(),
            progress: job.progress().cloned(),
            review: job.review().copied(),
            plan: job.plan().copied(),
            execution: job.execution().copied(),
            created_at_ms: job.created_at_ms(),
            updated_at_ms: job.updated_at_ms(),
        },
    }
}

fn map_json_rejection(_error: JsonRejection) -> ApiError {
    invalid_input("body", "must be valid JSON matching the job schema")
}

fn map_runtime_error(error: RuntimeError) -> ApiError {
    match error {
        RuntimeError::Store(error) => map_store_error(error),
        RuntimeError::CancellationConflict(state) => ApiError::new(
            StatusCode::CONFLICT,
            "job_state_conflict",
            "Job cannot be cancelled in its current state",
            Some(BTreeMap::from([("state".to_owned(), state.to_string())])),
        ),
        RuntimeError::EventHistoryExpired => ApiError::new(
            StatusCode::CONFLICT,
            "event_history_expired",
            "Requested job events are no longer available",
            None,
        ),
        RuntimeError::InvalidEventCursor => invalid_input(
            "last-event-id",
            "must identify an event issued by this server runtime",
        ),
        RuntimeError::EventSequenceExhausted => ApiError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "event_sequence_exhausted",
            "Job event service requires a restart",
            None,
        ),
    }
}

fn map_store_error(error: StoreError) -> ApiError {
    match error {
        StoreError::NotFound { id } => not_found(id),
        StoreError::InvalidTransition { .. } | StoreError::StateConflict { .. } => ApiError::new(
            StatusCode::CONFLICT,
            "job_state_conflict",
            "Job state changed before the request could be applied",
            None,
        ),
        _ => ApiError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "job_store_error",
            "Persistent job operation failed",
            None,
        ),
    }
}

fn not_found(id: i64) -> ApiError {
    ApiError::new(
        StatusCode::NOT_FOUND,
        "job_not_found",
        "Job not found",
        Some(BTreeMap::from([("job_id".to_owned(), id.to_string())])),
    )
}

fn invalid_input(field: &str, reason: &str) -> ApiError {
    ApiError::new(
        StatusCode::UNPROCESSABLE_ENTITY,
        "invalid_input",
        "Request fields are invalid",
        Some(BTreeMap::from([(field.to_owned(), reason.to_owned())])),
    )
}
