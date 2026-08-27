use std::{collections::BTreeMap, num::NonZeroI64};

use axum::{
    Json, Router,
    extract::{
        Path, Query, State,
        rejection::{JsonRejection, PathRejection, QueryRejection},
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
        artifacts::{CandidateArtifact, ConflictArtifact, OperationArtifact, WarningArtifact},
        model::{
            ExecutionSummary, JobInputDto, JobMediaKind, JobState, PlanSummary, ProgressSummary,
            ReviewDecisionDto, ReviewSummary,
        },
    },
    store::{JobId, JobRecord, StoreError},
};

const SCHEMA_VERSION: u8 = 1;

pub(crate) fn router(runtime: JobRuntime) -> Router {
    Router::new()
        .route("/jobs", get(list).post(create).fallback(get_or_post_only))
        .route("/jobs/{id}", get(get_job).fallback(get_only))
        .route("/jobs/{id}/cancel", post(cancel).fallback(post_only))
        .route("/jobs/{id}/retry", post(retry).fallback(post_only))
        .route(
            "/jobs/{id}/review",
            get(review_details).post(review).fallback(get_or_post_only),
        )
        .route("/jobs/{id}/plan", get(plan_details).fallback(get_only))
        .route("/jobs/{id}/execute", post(execute).fallback(post_only))
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

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReviewRequest {
    candidate_index: u64,
    accepted_conflict_indexes: Vec<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ExecuteRequest {
    approved: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ListJobsQuery {
    limit: Option<usize>,
    state: Option<JobState>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReviewDetailsQuery {
    candidate_index: Option<u64>,
}

#[derive(Debug, Serialize)]
struct JobListEnvelope {
    schema_version: u8,
    jobs: Vec<JobDto>,
    has_more: bool,
}

#[derive(Debug, Serialize)]
struct JobEnvelope {
    schema_version: u8,
    job: JobDto,
}

#[derive(Debug, Serialize)]
struct ReviewDetailsEnvelope {
    schema_version: u8,
    job_id: i64,
    selected_candidate_index: u64,
    candidates: Vec<CandidateArtifact>,
    candidates_truncated: bool,
    warnings: Vec<WarningArtifact>,
    warnings_truncated: bool,
    conflicts: Vec<ConflictArtifact>,
    conflicts_truncated: bool,
}

#[derive(Debug, Serialize)]
struct PlanDetailsEnvelope {
    schema_version: u8,
    job_id: i64,
    output_root: String,
    operations: Vec<OperationArtifact>,
    operations_truncated: bool,
    requires_approval: bool,
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
    review_decision: Option<ReviewDecisionDto>,
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

async fn list(
    State(runtime): State<JobRuntime>,
    query: Result<Query<ListJobsQuery>, QueryRejection>,
) -> Result<Json<JobListEnvelope>, ApiError> {
    let Query(query) =
        query.map_err(|_| invalid_input("query", "must contain a valid limit and job state"))?;
    let limit = query.limit.unwrap_or(50);
    if !(1..=100).contains(&limit) {
        return Err(invalid_input("limit", "must be between 1 and 100"));
    }
    let mut jobs = runtime
        .list(limit + 1, query.state)
        .await
        .map_err(map_runtime_error)?;
    let has_more = jobs.len() > limit;
    jobs.truncate(limit);
    Ok(Json(JobListEnvelope {
        schema_version: SCHEMA_VERSION,
        jobs: jobs.into_iter().map(job_dto).collect(),
        has_more,
    }))
}

async fn get_job(
    State(runtime): State<JobRuntime>,
    path: Result<Path<i64>, PathRejection>,
) -> Result<Json<JobEnvelope>, ApiError> {
    let id = extract_id(path)?;
    let job = runtime.get(id).await.map_err(map_runtime_error)?;
    Ok(Json(envelope(job)))
}

async fn review_details(
    State(runtime): State<JobRuntime>,
    path: Result<Path<i64>, PathRejection>,
    query: Result<Query<ReviewDetailsQuery>, QueryRejection>,
) -> Result<Json<ReviewDetailsEnvelope>, ApiError> {
    let id = extract_id(path)?;
    let Query(query) =
        query.map_err(|_| invalid_input("candidate_index", "must be a non-negative integer"))?;
    let (details, selected_candidate_index) = runtime
        .review_artifacts(id, query.candidate_index)
        .await
        .map_err(map_runtime_error)?;
    Ok(Json(ReviewDetailsEnvelope {
        schema_version: SCHEMA_VERSION,
        job_id: id.get(),
        selected_candidate_index,
        candidates: details.candidates,
        candidates_truncated: details.candidates_truncated,
        warnings: details.warnings,
        warnings_truncated: details.warnings_truncated,
        conflicts: details.conflicts,
        conflicts_truncated: details.conflicts_truncated,
    }))
}

async fn plan_details(
    State(runtime): State<JobRuntime>,
    path: Result<Path<i64>, PathRejection>,
) -> Result<Json<PlanDetailsEnvelope>, ApiError> {
    let id = extract_id(path)?;
    let (details, requires_approval) = runtime
        .plan_artifacts(id)
        .await
        .map_err(map_runtime_error)?;
    Ok(Json(PlanDetailsEnvelope {
        schema_version: SCHEMA_VERSION,
        job_id: id.get(),
        output_root: details.output_root,
        operations: details.operations,
        operations_truncated: details.operations_truncated,
        requires_approval,
    }))
}

async fn retry(
    State(runtime): State<JobRuntime>,
    path: Result<Path<i64>, PathRejection>,
) -> Result<Json<JobEnvelope>, ApiError> {
    let id = extract_id(path)?;
    let job = runtime.retry(id).await.map_err(map_runtime_error)?;
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

async fn review(
    State(runtime): State<JobRuntime>,
    path: Result<Path<i64>, PathRejection>,
    request: Result<Json<ReviewRequest>, JsonRejection>,
) -> Result<Json<JobEnvelope>, ApiError> {
    let id = extract_id(path)?;
    let Json(request) = request.map_err(map_json_rejection)?;
    if request.accepted_conflict_indexes.len() > 4096 {
        return Err(invalid_input(
            "accepted_conflict_indexes",
            "must contain no more than 4096 entries",
        ));
    }
    if !request
        .accepted_conflict_indexes
        .windows(2)
        .all(|indexes| indexes[0] < indexes[1])
    {
        return Err(invalid_input(
            "accepted_conflict_indexes",
            "must be strictly increasing without duplicates",
        ));
    }
    let decision =
        ReviewDecisionDto::new(request.candidate_index, request.accepted_conflict_indexes);
    let job = runtime
        .review(id, decision)
        .await
        .map_err(map_runtime_error)?;
    Ok(Json(envelope(job)))
}

async fn execute(
    State(runtime): State<JobRuntime>,
    path: Result<Path<i64>, PathRejection>,
    headers: HeaderMap,
    request: Result<Json<ExecuteRequest>, JsonRejection>,
) -> Result<Json<JobEnvelope>, ApiError> {
    let id = extract_id(path)?;
    let Json(request) = request.map_err(map_json_rejection)?;
    if !request.approved {
        return Err(invalid_input("approved", "must be true"));
    }
    let key = idempotency_key(&headers)?;
    let job = runtime.execute(id, key).await.map_err(map_runtime_error)?;
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

async fn get_or_post_only() -> Response {
    method_not_allowed("GET, HEAD, POST")
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

fn idempotency_key(headers: &HeaderMap) -> Result<&str, ApiError> {
    let value = headers
        .get("idempotency-key")
        .ok_or_else(|| invalid_input("idempotency-key", "header is required"))?
        .to_str()
        .map_err(|_| invalid_input("idempotency-key", "must be valid visible ASCII"))?;
    if value.is_empty()
        || value.len() > 256
        || !value.bytes().all(|byte| (0x21..=0x7e).contains(&byte))
    {
        return Err(invalid_input(
            "idempotency-key",
            "must contain 1 to 256 visible ASCII characters",
        ));
    }
    Ok(value)
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
        job: job_dto(job),
    }
}

fn job_dto(job: JobRecord) -> JobDto {
    JobDto {
        id: job.id().get(),
        input: job.input().clone(),
        state: job.state(),
        progress: job.progress().cloned(),
        review: job.review().copied(),
        review_decision: job.review_decision().cloned(),
        plan: job.plan().cloned(),
        execution: job.execution().copied(),
        created_at_ms: job.created_at_ms(),
        updated_at_ms: job.updated_at_ms(),
    }
}

fn map_json_rejection(_error: JsonRejection) -> ApiError {
    invalid_input("body", "must be valid JSON matching the job schema")
}

fn map_runtime_error(error: RuntimeError) -> ApiError {
    match error {
        RuntimeError::Store(error) => map_store_error(error),
        RuntimeError::FilesystemPolicy(_) => invalid_input(
            "input_path",
            "must resolve beneath a configured media root without symlink escapes",
        ),
        RuntimeError::CancellationConflict(state) => ApiError::new(
            StatusCode::CONFLICT,
            "job_state_conflict",
            "Job cannot be cancelled in its current state",
            Some(BTreeMap::from([("state".to_owned(), state.to_string())])),
        ),
        RuntimeError::ReviewConflict(state) => ApiError::new(
            StatusCode::CONFLICT,
            "job_state_conflict",
            "Job cannot be reviewed in its current state",
            Some(BTreeMap::from([("state".to_owned(), state.to_string())])),
        ),
        RuntimeError::ArtifactConflict(state) => ApiError::new(
            StatusCode::CONFLICT,
            "job_state_conflict",
            "Job artifacts are not available in its current state",
            Some(BTreeMap::from([("state".to_owned(), state.to_string())])),
        ),
        RuntimeError::ExecutionConflict(state) => ApiError::new(
            StatusCode::CONFLICT,
            "job_state_conflict",
            "Job cannot be executed in its current state",
            Some(BTreeMap::from([("state".to_owned(), state.to_string())])),
        ),
        RuntimeError::ConflictAcknowledgementMismatch { expected } => ApiError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "invalid_input",
            "Request fields are invalid",
            Some(BTreeMap::from([(
                "accepted_conflict_indexes".to_owned(),
                if expected == 0 {
                    "must be empty when the selected candidate has no conflicts".to_owned()
                } else {
                    format!("must acknowledge indexes 0 through {}", expected - 1)
                },
            )])),
        ),
        RuntimeError::ApprovalNotEnabled => {
            invalid_input("approved", "job input must enable apply before execution")
        }
        RuntimeError::StalePlan => ApiError::new(
            StatusCode::CONFLICT,
            "stale_plan",
            "Reviewed output plan is no longer current",
            None,
        ),
        RuntimeError::Flow(
            crate::jobs::JobFlowError::Sdk(fixer_sdk::SdkError::CandidateOutOfBounds { .. })
            | crate::jobs::JobFlowError::IndexOverflow,
        ) => invalid_input("candidate_index", "must identify an available candidate"),
        RuntimeError::WorkerFlowUnavailable
        | RuntimeError::ExecutionTaskClosed
        | RuntimeError::ExecutionShuttingDown
        | RuntimeError::CountOverflow
        | RuntimeError::Flow(_) => ApiError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "job_execution_error",
            "Job review or execution failed",
            None,
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
        StoreError::IdempotencyConflict { .. } => ApiError::new(
            StatusCode::CONFLICT,
            "idempotency_conflict",
            "Idempotency key does not match the existing execution request",
            None,
        ),
        StoreError::InvalidTransition { .. }
        | StoreError::StateConflict { .. }
        | StoreError::ExecutionReservationRequired { .. }
        | StoreError::ReservedExecutionRetry { .. } => ApiError::new(
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
