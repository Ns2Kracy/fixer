use std::{collections::BTreeMap, num::NonZeroI64, path::Path as FsPath};

use axum::{
    Json, Router,
    extract::{
        FromRef, Path, Query, State,
        rejection::{JsonRejection, PathRejection, QueryRejection},
    },
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response, Sse},
    routing::{get, post},
};
use fixer_writer_local::{PathTemplate, TemplateContext};
use serde::{Deserialize, Serialize};

use crate::{
    api::error::ApiError,
    fs_policy::FsPolicy,
    ingestion::{IngestionRuntime, RuleJobError, model::RulePlacement},
    jobs::{
        JobRuntime, RuntimeError,
        artifacts::{CandidateArtifact, ConflictArtifact, OperationArtifact, WarningArtifact},
        model::{
            ExecutionSummary, JobInputDto, JobMediaKind, JobOrganizationDto, JobState, PlanSummary,
            ProgressSummary, ReviewDecisionDto, ReviewSummary,
        },
    },
    store::{JobId, JobRecord, StoreError},
    workspace::{DirectoryRef, WorkspaceState},
};

const SCHEMA_VERSION: u8 = 1;

#[derive(Clone)]
struct JobApiState {
    runtime: JobRuntime,
    workspace: Option<WorkspaceState>,
    ingestion: Option<IngestionRuntime>,
}

impl FromRef<JobApiState> for JobRuntime {
    fn from_ref(state: &JobApiState) -> Self {
        state.runtime.clone()
    }
}

pub fn router(
    runtime: JobRuntime,
    workspace: Option<WorkspaceState>,
    ingestion: Option<IngestionRuntime>,
) -> Router {
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
        .with_state(JobApiState {
            runtime,
            workspace,
            ingestion,
        })
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum CreateJobRequest {
    RuleDirectory(CreateRuleDirectoryJobRequest),
    Directory(CreateDirectoryJobRequest),
    Path(CreatePathJobRequest),
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
enum RuleSelector {
    Matching,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CreateRuleDirectoryJobRequest {
    rule: RuleSelector,
    source: DirectoryRef,
    #[serde(default)]
    media_kind: Option<JobMediaKind>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CreateDirectoryJobRequest {
    media_kind: JobMediaKind,
    source: DirectoryRef,
    destination: DirectoryRef,
    placement: RulePlacement,
    apply: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CreatePathJobRequest {
    media_kind: JobMediaKind,
    input_path: String,
    apply: bool,
    #[serde(default)]
    organization: Option<JobOrganizationDto>,
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
    State(state): State<JobApiState>,
    request: Result<Json<CreateJobRequest>, JsonRejection>,
) -> Result<impl IntoResponse, ApiError> {
    let Json(request) = request.map_err(map_json_rejection)?;
    let job = match request {
        CreateJobRequest::RuleDirectory(request) => {
            let RuleSelector::Matching = request.rule;
            state
                .ingestion
                .as_ref()
                .ok_or_else(invalid_rule_job)?
                .create_rule_job(&request.source, request.media_kind)
                .await
                .map_err(|error| map_rule_job_error(&error))?
        }
        CreateJobRequest::Directory(request) => {
            let input = directory_job_input(state.workspace.as_ref(), &request)?;
            state
                .runtime
                .create(input)
                .await
                .map_err(map_runtime_error)?
        }
        CreateJobRequest::Path(request) => {
            let input = path_job_input(request)?;
            state
                .runtime
                .create(input)
                .await
                .map_err(map_runtime_error)?
        }
    };
    Ok((
        StatusCode::ACCEPTED,
        Json(envelope(&job, state.workspace.as_ref())),
    ))
}

fn path_job_input(request: CreatePathJobRequest) -> Result<JobInputDto, ApiError> {
    if request.input_path.trim().is_empty() {
        return Err(invalid_input("input_path", "must not be empty"));
    }
    let mut input = JobInputDto::new(request.media_kind, request.input_path, request.apply);
    if let Some(mut organization) = request.organization {
        validate_organization(&organization)?;
        // Only the ingestion supervisor can authorize automatic execution.
        organization.auto_execute = false;
        input = input.with_organization(organization);
    }
    Ok(input)
}

fn directory_job_input(
    workspace: Option<&WorkspaceState>,
    request: &CreateDirectoryJobRequest,
) -> Result<JobInputDto, ApiError> {
    let workspace = workspace.ok_or_else(invalid_directories)?;
    let source = workspace
        .resolve_directory(&request.source)
        .map_err(|_| invalid_directories())?;
    let destination = workspace
        .resolve_directory(&request.destination)
        .map_err(|_| invalid_directories())?;
    let policy = FsPolicy::new([&source.canonical_path, &destination.canonical_path])
        .map_err(|_| invalid_directories())?;
    let (source_path, destination_path) = policy
        .validate_directory_pair(&source.canonical_path, &destination.canonical_path)
        .map_err(|_| invalid_directories())?;
    Ok(JobInputDto::new(
        request.media_kind,
        source_path.to_string_lossy().into_owned(),
        request.apply,
    )
    .with_organization(JobOrganizationDto {
        destination_path: destination_path.to_string_lossy().into_owned(),
        placement: request.placement,
        path_template: None,
        origin_rule_id: None,
        auto_execute: false,
    }))
}

fn invalid_rule_job() -> ApiError {
    invalid_input(
        "rule",
        "matching requires folder rules and configured library roots",
    )
}

fn map_rule_job_error(error: &RuleJobError) -> ApiError {
    match error {
        RuleJobError::NoMatchingRule => ApiError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "no_matching_folder_rule",
            "No enabled Folder rule matches the selected directory",
            None,
        ),
        RuleJobError::NoWork => ApiError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "unrecognized_media",
            "The selected directory contains no recognizable media work",
            None,
        ),
        RuleJobError::MultipleWorks => ApiError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "multiple_media_works",
            "Select a directory containing exactly one media work",
            None,
        ),
        RuleJobError::AmbiguousMedia => ApiError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "ambiguous_media_kind",
            "Choose the media kind for the selected work",
            None,
        ),
        RuleJobError::InvalidMediaKindOverride => {
            invalid_input("media_kind", "must match the fixed Folder rule")
        }
        RuleJobError::Workspace(_) | RuleJobError::FilesystemPolicy(_) => invalid_directories(),
        RuleJobError::Discovery(_) | RuleJobError::Fingerprint(_) => {
            invalid_input("source", "could not be recognized as a single media work")
        }
        RuleJobError::Store(_) | RuleJobError::Join(_) | RuleJobError::Runtime(_) => ApiError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "rule_job_error",
            "The selected folder could not be queued",
            None,
        ),
    }
}

fn invalid_directories() -> ApiError {
    invalid_input(
        "directories",
        "must identify distinct, non-overlapping directories in configured roots",
    )
}

async fn list(
    State(state): State<JobApiState>,
    query: Result<Query<ListJobsQuery>, QueryRejection>,
) -> Result<Json<JobListEnvelope>, ApiError> {
    let Query(query) =
        query.map_err(|_| invalid_input("query", "must contain a valid limit and job state"))?;
    let limit = query.limit.unwrap_or(50);
    if !(1..=100).contains(&limit) {
        return Err(invalid_input("limit", "must be between 1 and 100"));
    }
    let mut jobs = state
        .runtime
        .list(limit + 1, query.state)
        .await
        .map_err(map_runtime_error)?;
    let has_more = jobs.len() > limit;
    jobs.truncate(limit);
    Ok(Json(JobListEnvelope {
        schema_version: SCHEMA_VERSION,
        jobs: jobs
            .iter()
            .map(|job| job_dto(job, state.workspace.as_ref()))
            .collect(),
        has_more,
    }))
}

async fn get_job(
    State(state): State<JobApiState>,
    path: Result<Path<i64>, PathRejection>,
) -> Result<Json<JobEnvelope>, ApiError> {
    let id = extract_id(path)?;
    let job = state.runtime.get(id).await.map_err(map_runtime_error)?;
    Ok(Json(envelope(&job, state.workspace.as_ref())))
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
    State(state): State<JobApiState>,
    path: Result<Path<i64>, PathRejection>,
) -> Result<Json<PlanDetailsEnvelope>, ApiError> {
    let id = extract_id(path)?;
    let (mut details, requires_approval) = state
        .runtime
        .plan_artifacts(id)
        .await
        .map_err(map_runtime_error)?;
    for operation in &mut details.operations {
        if let Some(source) = operation.source.as_mut() {
            *source = display_path(state.workspace.as_ref(), source);
        }
    }
    Ok(Json(PlanDetailsEnvelope {
        schema_version: SCHEMA_VERSION,
        job_id: id.get(),
        output_root: display_path(state.workspace.as_ref(), &details.output_root),
        operations: details.operations,
        operations_truncated: details.operations_truncated,
        requires_approval,
    }))
}

async fn retry(
    State(state): State<JobApiState>,
    path: Result<Path<i64>, PathRejection>,
) -> Result<Json<JobEnvelope>, ApiError> {
    let id = extract_id(path)?;
    let job = state.runtime.retry(id).await.map_err(map_runtime_error)?;
    Ok(Json(envelope(&job, state.workspace.as_ref())))
}

async fn cancel(
    State(state): State<JobApiState>,
    path: Result<Path<i64>, PathRejection>,
) -> Result<Json<JobEnvelope>, ApiError> {
    let id = extract_id(path)?;
    let job = state.runtime.cancel(id).await.map_err(map_runtime_error)?;
    Ok(Json(envelope(&job, state.workspace.as_ref())))
}

async fn review(
    State(state): State<JobApiState>,
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
    let job = state
        .runtime
        .review(id, decision)
        .await
        .map_err(map_runtime_error)?;
    Ok(Json(envelope(&job, state.workspace.as_ref())))
}

async fn execute(
    State(state): State<JobApiState>,
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
    let job = state
        .runtime
        .execute(id, key)
        .await
        .map_err(map_runtime_error)?;
    Ok(Json(envelope(&job, state.workspace.as_ref())))
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
        .map_err(|error| map_store_error(&error))?
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

fn envelope(job: &JobRecord, workspace: Option<&WorkspaceState>) -> JobEnvelope {
    JobEnvelope {
        schema_version: SCHEMA_VERSION,
        job: job_dto(job, workspace),
    }
}

fn job_dto(job: &JobRecord, workspace: Option<&WorkspaceState>) -> JobDto {
    JobDto {
        id: job.id().get(),
        input: display_input(job.input(), workspace),
        state: job.state(),
        progress: job.progress().cloned(),
        review: job.review().cloned(),
        review_decision: job.review_decision().cloned(),
        plan: job.plan().cloned(),
        execution: job
            .execution()
            .cloned()
            .map(ExecutionSummary::without_operations),
        created_at_ms: job.created_at_ms(),
        updated_at_ms: job.updated_at_ms(),
    }
}

fn display_input(input: &JobInputDto, workspace: Option<&WorkspaceState>) -> JobInputDto {
    let mut displayed = JobInputDto::new(
        input.media_kind(),
        display_path(workspace, input.input_path()),
        input.apply(),
    )
    .with_selection(input.selection().clone());
    if input.unattended() {
        displayed = displayed.with_unattended();
    }
    if let Some(run_id) = input.correction_of() {
        displayed = displayed.with_correction_of(run_id);
    }
    if let Some(run_id) = input.retry_of() {
        displayed = displayed.with_retry_of(run_id);
    }
    if let Some(organization) = input.organization() {
        let mut organization = organization.clone();
        organization.destination_path = display_path(workspace, &organization.destination_path);
        displayed = displayed.with_organization(organization);
    }
    displayed
}

fn display_path(workspace: Option<&WorkspaceState>, path: &str) -> String {
    workspace.map_or_else(
        || "Unavailable item".to_owned(),
        |workspace| workspace.display_path(FsPath::new(path)),
    )
}

fn validate_organization(organization: &JobOrganizationDto) -> Result<(), ApiError> {
    if organization.destination_path.trim().is_empty()
        || organization.destination_path.len() > 4096
        || organization.destination_path.chars().any(char::is_control)
    {
        return Err(invalid_input(
            "organization.destination_path",
            "must contain between 1 and 4096 bytes without control characters",
        ));
    }
    if organization.origin_rule_id.is_some_and(|id| id <= 0) {
        return Err(invalid_input(
            "organization.origin_rule_id",
            "must be a positive integer when provided",
        ));
    }
    if let Some(template) = organization.path_template.as_deref() {
        if template.len() > 4096 || template.chars().any(char::is_control) {
            return Err(invalid_input(
                "organization.path_template",
                "must contain no more than 4096 bytes without control characters",
            ));
        }
        let context =
            TemplateContext::preview("Example", "example-id", Some(2000), Some("Edition".into()))
                .map_err(|_| {
                invalid_input("organization.path_template", "template validation failed")
            })?;
        PathTemplate::new(template)
            .and_then(|template| template.render(&context))
            .map_err(|_| {
                invalid_input(
                    "organization.path_template",
                    "must render to a safe relative output path",
                )
            })?;
    }
    Ok(())
}

fn map_json_rejection(_error: JsonRejection) -> ApiError {
    invalid_input("body", "must be valid JSON matching the job schema")
}

pub(super) fn map_runtime_error(error: RuntimeError) -> ApiError {
    match error {
        RuntimeError::Store(error) => map_store_error(&error),
        RuntimeError::FilesystemPolicy(_) => invalid_input(
            "input_path",
            "must resolve beneath a configured media root without symlink escapes",
        ),
        RuntimeError::OrganizationFilesystemPolicy(_) => invalid_input(
            "organization.destination_path",
            "must resolve to a configured media directory without symlink escapes",
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
        RuntimeError::QueueUnavailable => ApiError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "scrape_queue_unavailable",
            "Scrape queue is unavailable",
            None,
        ),
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

fn map_store_error(error: &StoreError) -> ApiError {
    match error {
        StoreError::NotFound { id } => not_found(*id),
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
