use std::{collections::BTreeMap, path::Path};

use axum::{
    Json, Router,
    extract::{FromRef, Path as AxumPath, Query, State, rejection::JsonRejection},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
};
use fixer_core::{MediaKind, ProviderTarget, ScrapeSelection};
use serde::{Deserialize, Serialize};

use crate::{
    api::error::ApiError,
    jobs::{
        JobRuntime,
        model::{ExecutionSummary, JobInputDto, JobMediaKind, JobState},
    },
    store::{JobId, JobRecord},
    workspace::{DirectoryRef, WorkspaceState},
};

const SCHEMA_VERSION: u8 = 1;

#[derive(Clone)]
struct RunApiState {
    runtime: JobRuntime,
    workspace: Option<WorkspaceState>,
}

impl FromRef<RunApiState> for JobRuntime {
    fn from_ref(state: &RunApiState) -> Self {
        state.runtime.clone()
    }
}

pub fn router(runtime: JobRuntime, workspace: Option<WorkspaceState>) -> Router {
    Router::new()
        .route(
            "/scrape-runs",
            get(list).post(create).fallback(get_or_post_only),
        )
        .route("/scrape-runs/{id}", get(get_run).fallback(get_only))
        .route("/scrape-runs/{id}/retry", post(retry).fallback(post_only))
        .route("/scrape-runs/{id}/cancel", post(cancel).fallback(post_only))
        .with_state(RunApiState { runtime, workspace })
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CreateRunRequest {
    #[serde(default)]
    media_kind: Option<JobMediaKind>,
    #[serde(default)]
    input_path: Option<String>,
    #[serde(default)]
    source: Option<DirectoryRef>,
    #[serde(default)]
    target: Option<ProviderTarget>,
    #[serde(default)]
    correction_of: Option<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ListRunsQuery {
    limit: Option<usize>,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
enum RunStatus {
    Queued,
    Running,
    Succeeded,
    Failed,
    Cancelled,
}

#[derive(Debug, Serialize)]
struct RunListEnvelope {
    schema_version: u8,
    runs: Vec<RunDto>,
    has_more: bool,
}

#[derive(Debug, Serialize)]
struct RunEnvelope {
    schema_version: u8,
    run: RunDto,
}

#[derive(Debug, Serialize)]
struct RunDto {
    id: i64,
    item_name: String,
    media_kind: JobMediaKind,
    status: RunStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    requested_target: Option<ProviderTarget>,
    #[serde(skip_serializing_if = "Option::is_none")]
    selected_target: Option<ProviderTarget>,
    #[serde(skip_serializing_if = "Option::is_none")]
    correction_of: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    retry_of: Option<i64>,
    candidate_count: u64,
    conflict_count: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    execution: Option<ExecutionSummary>,
    created_at_ms: i64,
    updated_at_ms: i64,
}

async fn create(
    State(state): State<RunApiState>,
    request: Result<Json<CreateRunRequest>, JsonRejection>,
) -> Result<impl IntoResponse, ApiError> {
    let Json(request) = request.map_err(|_| invalid("body", "must match the scrape run schema"))?;
    let (media_kind, input_path) =
        resolve_input(&state.runtime, state.workspace.as_ref(), &request).await?;
    validate_input_path(&input_path)?;
    let selection = validated_selection(media_kind, request.target)?;
    let mut input = JobInputDto::new(media_kind, input_path, true)
        .with_unattended()
        .with_selection(selection);
    if let Some(parent) = request.correction_of {
        input = input.with_correction_of(parent);
    }
    let (run, created) = state
        .runtime
        .create_run(input)
        .await
        .map_err(super::jobs::map_runtime_error)?;
    let status = if created {
        StatusCode::ACCEPTED
    } else {
        StatusCode::OK
    };
    Ok((status, Json(envelope(&run))))
}

async fn resolve_input(
    runtime: &JobRuntime,
    workspace: Option<&WorkspaceState>,
    request: &CreateRunRequest,
) -> Result<(JobMediaKind, String), ApiError> {
    let Some(parent_id) = request.correction_of else {
        let media_kind = request
            .media_kind
            .ok_or_else(|| invalid("media_kind", "is required for a new scrape"))?;
        let input_path = match (&request.source, &request.input_path) {
            (Some(_), Some(_)) => {
                return Err(invalid("source", "cannot be combined with input_path"));
            }
            (Some(source), None) => workspace
                .ok_or_else(|| invalid("source", "workspace browsing is unavailable"))?
                .resolve_directory(source)
                .map_err(|_| invalid("source", "must identify an available media directory"))?
                .canonical_path
                .to_string_lossy()
                .into_owned(),
            (None, Some(path)) => path.clone(),
            (None, None) => {
                return Err(invalid("source", "is required for a new scrape"));
            }
        };
        return Ok((media_kind, input_path));
    };
    if request.source.is_some() {
        return Err(invalid("source", "must be omitted for a correction"));
    }
    if request.target.is_none() {
        return Err(invalid(
            "target",
            "is required when correcting an earlier scrape",
        ));
    }
    let parent = runtime
        .get(parse_id(parent_id)?)
        .await
        .map_err(super::jobs::map_runtime_error)?;
    if parent.state() != JobState::Completed
        || parent
            .execution()
            .is_none_or(|execution| execution.operations().is_empty())
    {
        return Err(ApiError::new(
            StatusCode::CONFLICT,
            "run_not_correctable",
            "Only a completed scrape with recorded outputs can be corrected",
            None,
        ));
    }
    if request
        .media_kind
        .is_some_and(|kind| kind != parent.input().media_kind())
    {
        return Err(invalid("media_kind", "must match the corrected scrape"));
    }
    if request
        .input_path
        .as_deref()
        .is_some_and(|path| path != parent.input().input_path())
    {
        return Err(invalid("input_path", "must match the corrected scrape"));
    }
    Ok((
        parent.input().media_kind(),
        parent.input().input_path().to_owned(),
    ))
}

async fn list(
    State(runtime): State<JobRuntime>,
    Query(query): Query<ListRunsQuery>,
) -> Result<Json<RunListEnvelope>, ApiError> {
    let limit = query.limit.unwrap_or(50);
    if !(1..=100).contains(&limit) {
        return Err(invalid("limit", "must be between 1 and 100"));
    }
    let mut runs = runtime
        .list(10_000, None)
        .await
        .map_err(super::jobs::map_runtime_error)?
        .into_iter()
        .filter(|job| job.input().unattended())
        .collect::<Vec<_>>();
    let has_more = runs.len() > limit;
    runs.truncate(limit);
    Ok(Json(RunListEnvelope {
        schema_version: SCHEMA_VERSION,
        runs: runs.iter().map(run_dto).collect(),
        has_more,
    }))
}

async fn get_run(
    State(runtime): State<JobRuntime>,
    AxumPath(id): AxumPath<i64>,
) -> Result<Json<RunEnvelope>, ApiError> {
    let run = runtime
        .get(parse_id(id)?)
        .await
        .map_err(super::jobs::map_runtime_error)?;
    ensure_run(&run)?;
    Ok(Json(envelope(&run)))
}

async fn retry(
    State(runtime): State<JobRuntime>,
    AxumPath(id): AxumPath<i64>,
) -> Result<impl IntoResponse, ApiError> {
    let original = runtime
        .get(parse_id(id)?)
        .await
        .map_err(super::jobs::map_runtime_error)?;
    ensure_run(&original)?;
    if !matches!(
        original.state(),
        JobState::AwaitingConfirmation
            | JobState::Failed
            | JobState::Interrupted
            | JobState::Cancelled
    ) {
        return Err(ApiError::new(
            StatusCode::CONFLICT,
            "run_state_conflict",
            "Run can only be retried after it stops unsuccessfully",
            None,
        ));
    }
    let mut input = original.input().clone().with_unattended();
    if original.execution().is_some_and(|execution| {
        execution
            .operations()
            .iter()
            .any(|operation| operation.outcome() == fixer_core::OperationOutcome::Succeeded)
    }) {
        input = input.with_retry_of(id);
    }
    let (run, created) = runtime
        .create_run(input)
        .await
        .map_err(super::jobs::map_runtime_error)?;
    let status = if created {
        StatusCode::ACCEPTED
    } else {
        StatusCode::OK
    };
    Ok((status, Json(envelope(&run))))
}

async fn cancel(
    State(runtime): State<JobRuntime>,
    AxumPath(id): AxumPath<i64>,
) -> Result<Json<RunEnvelope>, ApiError> {
    let id = parse_id(id)?;
    ensure_run(
        &runtime
            .get(id)
            .await
            .map_err(super::jobs::map_runtime_error)?,
    )?;
    let run = runtime
        .cancel(id)
        .await
        .map_err(super::jobs::map_runtime_error)?;
    Ok(Json(envelope(&run)))
}

fn validated_selection(
    media_kind: JobMediaKind,
    target: Option<ProviderTarget>,
) -> Result<ScrapeSelection, ApiError> {
    let Some(target) = target else {
        return Ok(ScrapeSelection::Automatic);
    };
    let target = if target.provider().as_str() == "tmdb" {
        if target.external_id().namespace != "tmdb" {
            return Err(invalid("target", "TMDB IDs must use the tmdb namespace"));
        }
        ProviderTarget::tmdb(target.media_kind(), &target.external_id().value)
    } else {
        ProviderTarget::new(
            target.media_kind(),
            target.provider().clone(),
            target.external_id().clone(),
        )
    }
    .map_err(|_| invalid("target", "must contain a valid provider and external ID"))?;
    if target.media_kind() != core_media_kind(media_kind) {
        return Err(invalid("target", "media kind must match the scrape"));
    }
    Ok(ScrapeSelection::Exact(target))
}

const fn core_media_kind(value: JobMediaKind) -> MediaKind {
    match value {
        JobMediaKind::Anime => MediaKind::Anime,
        JobMediaKind::Book => MediaKind::Book,
        JobMediaKind::Movie => MediaKind::Movie,
        JobMediaKind::Music => MediaKind::Music,
        JobMediaKind::Television => MediaKind::Television,
    }
}

fn envelope(run: &JobRecord) -> RunEnvelope {
    RunEnvelope {
        schema_version: SCHEMA_VERSION,
        run: run_dto(run),
    }
}

fn run_dto(run: &JobRecord) -> RunDto {
    let requested_target = match run.input().selection() {
        ScrapeSelection::Automatic => None,
        ScrapeSelection::Exact(target) => Some(target.clone()),
    };
    RunDto {
        id: run.id().get(),
        item_name: item_name(run.input().input_path()),
        media_kind: run.input().media_kind(),
        status: run_status(run.state()),
        requested_target,
        selected_target: run
            .review()
            .and_then(|review| review.selected_target().cloned()),
        correction_of: run.input().correction_of(),
        retry_of: run.input().retry_of(),
        candidate_count: run
            .review()
            .map_or(0, crate::jobs::model::ReviewSummary::candidate_count),
        conflict_count: run
            .review()
            .map_or(0, crate::jobs::model::ReviewSummary::conflict_count),
        execution: run.execution().cloned(),
        created_at_ms: run.created_at_ms(),
        updated_at_ms: run.updated_at_ms(),
    }
}

const fn run_status(state: JobState) -> RunStatus {
    match state {
        JobState::Queued => RunStatus::Queued,
        JobState::Scanning
        | JobState::Searching
        | JobState::Resolving
        | JobState::Planning
        | JobState::Writing => RunStatus::Running,
        JobState::AwaitingConfirmation | JobState::Failed | JobState::Interrupted => {
            RunStatus::Failed
        }
        JobState::Completed => RunStatus::Succeeded,
        JobState::Cancelled => RunStatus::Cancelled,
    }
}

fn item_name(path: &str) -> String {
    Path::new(path)
        .file_name()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .unwrap_or("Media item")
        .to_owned()
}

fn validate_input_path(path: &str) -> Result<(), ApiError> {
    if path.trim().is_empty() || path.len() > 4096 || path.chars().any(char::is_control) {
        Err(invalid(
            "input_path",
            "must contain between 1 and 4096 bytes without control characters",
        ))
    } else {
        Ok(())
    }
}

fn ensure_run(record: &JobRecord) -> Result<(), ApiError> {
    if record.input().unattended() {
        Ok(())
    } else {
        Err(ApiError::new(
            StatusCode::NOT_FOUND,
            "run_not_found",
            "Scrape run was not found",
            None,
        ))
    }
}

fn parse_id(value: i64) -> Result<JobId, ApiError> {
    JobId::from_database(value).map_err(|_| invalid("id", "must be a positive integer"))
}

fn invalid(field: &str, message: &str) -> ApiError {
    ApiError::new(
        StatusCode::UNPROCESSABLE_ENTITY,
        "invalid_input",
        "Request fields are invalid",
        Some(BTreeMap::from([(field.to_owned(), message.to_owned())])),
    )
}

async fn get_only() -> ApiError {
    crate::api::error::method_not_allowed().await
}

async fn post_only() -> ApiError {
    crate::api::error::method_not_allowed().await
}

async fn get_or_post_only() -> ApiError {
    crate::api::error::method_not_allowed().await
}
