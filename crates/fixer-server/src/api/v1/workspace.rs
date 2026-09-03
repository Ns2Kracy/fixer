use std::collections::BTreeMap;

use axum::{
    Json, Router,
    extract::{
        Path, Query, State,
        rejection::{JsonRejection, PathRejection, QueryRejection},
    },
    http::StatusCode,
    routing::{get, post},
};
use fixer_writer_local::{ContentTemplate, PathTemplate, TemplateContext, TemplateError};
use serde::{Deserialize, Serialize};

use crate::{
    WorkspaceState,
    api::error::ApiError,
    workspace::{
        LibraryEntry, ProviderProbeResult, RootSummary, SearchMatch, WorkspaceSettingsInput,
        WorkspaceSettingsSnapshot,
    },
};

const SCHEMA_VERSION: u32 = 1;

pub fn router(state: WorkspaceState) -> Router {
    Router::new()
        .route(
            "/settings",
            get(get_settings)
                .put(update_settings)
                .fallback(crate::api::error::method_not_allowed),
        )
        .route(
            "/library/roots",
            get(roots).fallback(crate::api::error::method_not_allowed),
        )
        .route(
            "/library",
            get(list_library).fallback(crate::api::error::method_not_allowed),
        )
        .route(
            "/search",
            get(search).fallback(crate::api::error::method_not_allowed),
        )
        .route(
            "/providers/{provider}/test",
            post(test_provider).fallback(crate::api::error::method_not_allowed),
        )
        .route(
            "/templates/preview",
            post(preview_template).fallback(crate::api::error::method_not_allowed),
        )
        .with_state(state)
}

#[derive(Serialize)]
struct SettingsEnvelope {
    schema_version: u32,
    settings: WorkspaceSettingsSnapshot,
}

#[derive(Serialize)]
struct RootsEnvelope {
    schema_version: u32,
    roots: Vec<RootSummary>,
}

#[derive(Serialize)]
struct LibraryEnvelope {
    schema_version: u32,
    root_id: String,
    path: String,
    entries: Vec<LibraryEntry>,
    truncated: bool,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum SearchMediaKind {
    Movie,
    Television,
    Anime,
    Music,
    Book,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LibraryQuery {
    root_id: String,
    #[serde(default)]
    path: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SearchQuery {
    media_kind: SearchMediaKind,
    query: String,
    #[serde(default = "default_search_limit")]
    limit: usize,
}

#[derive(Serialize)]
struct SearchEnvelope {
    schema_version: u32,
    media_kind: SearchMediaKind,
    results: Vec<SearchMatch>,
    truncated: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EmptyRequest {}

#[derive(Serialize)]
struct ProviderProbeEnvelope {
    schema_version: u32,
    provider: String,
    ok: bool,
    category: &'static str,
    message: &'static str,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TemplatePreviewRequest {
    path_template: String,
    content_template: String,
    sample: TemplateSample,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TemplateSample {
    title: String,
    id: String,
    year: Option<u16>,
    edition: Option<String>,
}

#[derive(Serialize)]
struct TemplatePreviewEnvelope {
    schema_version: u32,
    path: String,
    content: String,
    content_bytes: usize,
}

const fn default_search_limit() -> usize {
    25
}

async fn get_settings(State(state): State<WorkspaceState>) -> Json<SettingsEnvelope> {
    Json(settings_envelope(state.settings()))
}

async fn update_settings(
    State(state): State<WorkspaceState>,
    request: Result<Json<WorkspaceSettingsInput>, JsonRejection>,
) -> Result<Json<SettingsEnvelope>, ApiError> {
    let Json(request) = request.map_err(map_settings_json_rejection)?;
    let settings = state
        .update_settings(request)
        .await
        .map_err(|error| map_state_error(&error))?;
    Ok(Json(settings_envelope(settings)))
}

async fn roots(State(state): State<WorkspaceState>) -> Json<RootsEnvelope> {
    Json(RootsEnvelope {
        schema_version: SCHEMA_VERSION,
        roots: state.roots(),
    })
}

async fn list_library(
    State(state): State<WorkspaceState>,
    query: Result<Query<LibraryQuery>, QueryRejection>,
) -> Result<Json<LibraryEnvelope>, ApiError> {
    let Query(query) = query.map_err(|_| {
        invalid_input(
            "query",
            "must contain a configured root_id and an optional relative path",
        )
    })?;
    let listing = tokio::task::spawn_blocking(move || state.list(&query.root_id, &query.path))
        .await
        .map_err(map_blocking_task_error)?
        .map_err(|error| map_state_error(&error))?;
    Ok(Json(LibraryEnvelope {
        schema_version: SCHEMA_VERSION,
        root_id: listing.root_id,
        path: listing.path,
        entries: listing.entries,
        truncated: listing.truncated,
    }))
}

async fn search(
    State(state): State<WorkspaceState>,
    query: Result<Query<SearchQuery>, QueryRejection>,
) -> Result<Json<SearchEnvelope>, ApiError> {
    let Query(query) = query.map_err(|_| {
        invalid_input(
            "query",
            "must contain a supported media_kind, query text, and optional limit",
        )
    })?;
    let matches = tokio::task::spawn_blocking(move || state.search(&query.query, query.limit))
        .await
        .map_err(map_blocking_task_error)?
        .map_err(|error| map_state_error(&error))?;
    Ok(Json(SearchEnvelope {
        schema_version: SCHEMA_VERSION,
        media_kind: query.media_kind,
        results: matches.results,
        truncated: matches.truncated,
    }))
}

async fn test_provider(
    State(state): State<WorkspaceState>,
    path: Result<Path<String>, PathRejection>,
    request: Result<Json<EmptyRequest>, JsonRejection>,
) -> Result<Json<ProviderProbeEnvelope>, ApiError> {
    let Path(provider) =
        path.map_err(|_| invalid_input("provider", "must be a valid provider ID"))?;
    let Json(_) = request.map_err(|_| invalid_input("body", "must be an empty JSON object"))?;
    let result = state
        .probe_provider(&provider)
        .await
        .map_err(|error| map_state_error(&error))?;
    Ok(Json(provider_probe_envelope(result)))
}

async fn preview_template(
    request: Result<Json<TemplatePreviewRequest>, JsonRejection>,
) -> Result<Json<TemplatePreviewEnvelope>, ApiError> {
    let Json(request) = request.map_err(|_| {
        invalid_input(
            "body",
            "must be valid JSON matching the template preview schema",
        )
    })?;
    let context = TemplateContext::preview(
        request.sample.title,
        request.sample.id,
        request.sample.year,
        request.sample.edition,
    )
    .map_err(|error| map_template_error(&error))?;
    let path = PathTemplate::new(request.path_template)
        .and_then(|template| template.render(&context))
        .map_err(|error| map_template_error(&error))?;
    let rendered_content = ContentTemplate::new(request.content_template)
        .and_then(|template| template.render(&context))
        .map_err(|error| map_template_error(&error))?;
    let path = path
        .to_str()
        .ok_or_else(|| invalid_template("rendered path is not valid UTF-8"))?
        .replace('\\', "/");
    let content_bytes = rendered_content.len();
    Ok(Json(TemplatePreviewEnvelope {
        schema_version: SCHEMA_VERSION,
        path,
        content: rendered_content,
        content_bytes,
    }))
}

const fn settings_envelope(settings: WorkspaceSettingsSnapshot) -> SettingsEnvelope {
    SettingsEnvelope {
        schema_version: SCHEMA_VERSION,
        settings,
    }
}

fn provider_probe_envelope(result: ProviderProbeResult) -> ProviderProbeEnvelope {
    ProviderProbeEnvelope {
        schema_version: SCHEMA_VERSION,
        provider: result.provider,
        ok: result.ok,
        category: result.category,
        message: result.message,
    }
}

fn map_blocking_task_error(_error: tokio::task::JoinError) -> ApiError {
    ApiError::new(
        StatusCode::INTERNAL_SERVER_ERROR,
        "workspace_error",
        "Workspace operation failed",
        None,
    )
}

fn map_settings_json_rejection(_error: JsonRejection) -> ApiError {
    invalid_input("body", "must be valid JSON matching the settings schema")
}

fn map_state_error(error: &crate::WorkspaceStateError) -> ApiError {
    use crate::WorkspaceStateError;

    match error {
        WorkspaceStateError::InvalidInput { field, reason } => invalid_input(field, reason),
        WorkspaceStateError::RootNotFound => ApiError::new(
            StatusCode::NOT_FOUND,
            "library_root_not_found",
            "Configured library root was not found",
            None,
        ),
        WorkspaceStateError::PathNotFound => ApiError::new(
            StatusCode::NOT_FOUND,
            "library_path_not_found",
            "Library path was not found",
            None,
        ),
        WorkspaceStateError::InvalidPath => ApiError::new(
            StatusCode::BAD_REQUEST,
            "invalid_library_path",
            "Library path must be relative to a configured root",
            None,
        ),
        WorkspaceStateError::NotDirectory => ApiError::new(
            StatusCode::BAD_REQUEST,
            "library_path_not_directory",
            "Library path is not a directory",
            None,
        ),
        WorkspaceStateError::ProviderNotFound => ApiError::new(
            StatusCode::NOT_FOUND,
            "provider_not_found",
            "Provider was not found",
            None,
        ),
        WorkspaceStateError::ProbeConfiguration => ApiError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "provider_configuration_error",
            "Provider connectivity test could not be configured",
            None,
        ),
        WorkspaceStateError::InvalidRoot
        | WorkspaceStateError::Inspect
        | WorkspaceStateError::SettingsTask
        | WorkspaceStateError::SettingsPersistence(_) => ApiError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "workspace_error",
            "Workspace operation failed",
            None,
        ),
    }
}

fn map_template_error(error: &TemplateError) -> ApiError {
    invalid_template(match error {
        TemplateError::InvalidSyntax(_) => "template syntax is invalid",
        TemplateError::UnsupportedVariable(_) => "template contains an unsupported variable",
        TemplateError::UnsupportedFilter(_) => "template contains an unsupported filter",
        TemplateError::MissingVariable(_) => "sample is missing a required template variable",
        TemplateError::UnsafePath(_) => "rendered output path is unsafe",
        TemplateError::Locale(_) => "template locale policy is invalid",
    })
}

fn invalid_template(reason: &str) -> ApiError {
    ApiError::new(
        StatusCode::UNPROCESSABLE_ENTITY,
        "invalid_template",
        "Template preview is invalid",
        Some(BTreeMap::from([("template".to_owned(), reason.to_owned())])),
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
