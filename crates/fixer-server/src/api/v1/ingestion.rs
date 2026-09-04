use std::collections::BTreeMap;

use axum::{
    Json, Router,
    extract::{
        Path, State,
        rejection::{JsonRejection, PathRejection},
    },
    http::StatusCode,
    routing::get,
};
use fixer_writer_local::{PathTemplate, TemplateContext};
use serde::{Deserialize, Serialize};

use crate::{
    IngestionRuntime,
    api::error::ApiError,
    ingestion::{
        IngestionRuntimeError,
        model::{
            IngestionModelError, IngestionRule, IngestionRuleId, IngestionRuleInput, MediaKindMode,
            RuleDirectory, RulePlacement, RuleStatus,
        },
    },
    workspace::DirectoryRef,
};

const SCHEMA_VERSION: u32 = 1;

pub fn router(runtime: IngestionRuntime) -> Router {
    Router::new()
        .route(
            "/ingestion-rules",
            get(list_rules)
                .post(create_rule)
                .fallback(crate::api::error::method_not_allowed),
        )
        .route(
            "/ingestion-rules/{id}",
            axum::routing::put(update_rule)
                .delete(delete_rule)
                .fallback(crate::api::error::method_not_allowed),
        )
        .route(
            "/ingestion-rules/{id}/scan",
            axum::routing::post(scan_rule).fallback(crate::api::error::method_not_allowed),
        )
        .with_state(runtime)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RuleRequest {
    name: String,
    source: DirectoryRef,
    destination: DirectoryRef,
    media_kind_mode: MediaKindMode,
    placement: RulePlacement,
    #[serde(default)]
    path_template_override: Option<String>,
    #[serde(default = "enabled_by_default")]
    enabled: bool,
}

#[derive(Serialize)]
struct RuleDirectoryDto {
    root_id: String,
    path: String,
}

#[derive(Serialize)]
struct RuleDto {
    id: i64,
    name: String,
    source: RuleDirectoryDto,
    destination: RuleDirectoryDto,
    media_kind_mode: MediaKindMode,
    placement: RulePlacement,
    path_template_override: Option<String>,
    enabled: bool,
    status: RuleStatus,
    last_error: Option<String>,
    created_at_ms: i64,
    updated_at_ms: i64,
}

#[derive(Serialize)]
struct RuleEnvelope {
    schema_version: u32,
    rule: RuleDto,
}

#[derive(Serialize)]
struct RuleListEnvelope {
    schema_version: u32,
    rules: Vec<RuleDto>,
}

#[derive(Serialize)]
struct ScanEnvelope {
    schema_version: u32,
    rule_id: i64,
    requested: bool,
}

const fn enabled_by_default() -> bool {
    true
}

async fn list_rules(
    State(runtime): State<IngestionRuntime>,
) -> Result<Json<RuleListEnvelope>, ApiError> {
    let rules = runtime
        .list_rules()
        .await
        .map_err(map_runtime_error)?
        .iter()
        .map(rule_dto)
        .collect();
    Ok(Json(RuleListEnvelope {
        schema_version: SCHEMA_VERSION,
        rules,
    }))
}

async fn create_rule(
    State(runtime): State<IngestionRuntime>,
    request: Result<Json<RuleRequest>, JsonRejection>,
) -> Result<(StatusCode, Json<RuleEnvelope>), ApiError> {
    let Json(request) = request.map_err(map_json_rejection)?;
    let input = rule_input(&runtime, request)?;
    let rule = runtime
        .create_rule(input)
        .await
        .map_err(map_runtime_error)?;
    Ok((StatusCode::CREATED, Json(rule_envelope(&rule))))
}

async fn update_rule(
    State(runtime): State<IngestionRuntime>,
    path: Result<Path<i64>, PathRejection>,
    request: Result<Json<RuleRequest>, JsonRejection>,
) -> Result<Json<RuleEnvelope>, ApiError> {
    let id = extract_id(path)?;
    let Json(request) = request.map_err(map_json_rejection)?;
    let input = rule_input(&runtime, request)?;
    let rule = runtime
        .update_rule(id, input)
        .await
        .map_err(map_runtime_error)?
        .ok_or_else(|| not_found(id.get()))?;
    Ok(Json(rule_envelope(&rule)))
}

async fn delete_rule(
    State(runtime): State<IngestionRuntime>,
    path: Result<Path<i64>, PathRejection>,
) -> Result<StatusCode, ApiError> {
    let id = extract_id(path)?;
    if !runtime.delete_rule(id).await.map_err(map_runtime_error)? {
        return Err(not_found(id.get()));
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn scan_rule(
    State(runtime): State<IngestionRuntime>,
    path: Result<Path<i64>, PathRejection>,
) -> Result<(StatusCode, Json<ScanEnvelope>), ApiError> {
    let id = extract_id(path)?;
    if !runtime
        .request_rescan(id)
        .await
        .map_err(map_runtime_error)?
    {
        return Err(not_found(id.get()));
    }
    Ok((
        StatusCode::ACCEPTED,
        Json(ScanEnvelope {
            schema_version: SCHEMA_VERSION,
            rule_id: id.get(),
            requested: true,
        }),
    ))
}

fn rule_input(
    runtime: &IngestionRuntime,
    request: RuleRequest,
) -> Result<IngestionRuleInput, ApiError> {
    if let Some(template) = request.path_template_override.as_deref() {
        validate_template(template)?;
    }
    let (source, destination) = runtime
        .validate_directories(&request.source, &request.destination)
        .map_err(map_runtime_error)?;
    let mut input = IngestionRuleInput::new(
        request.name,
        source,
        destination,
        request.media_kind_mode,
        request.placement,
    )
    .map_err(|error| map_model_error(&error))?
    .with_enabled(request.enabled);
    if let Some(template) = request.path_template_override {
        input = input
            .with_path_template_override(template)
            .map_err(|error| map_model_error(&error))?;
    }
    Ok(input)
}

fn validate_template(template: &str) -> Result<(), ApiError> {
    let context =
        TemplateContext::preview("Example", "example-id", Some(2000), Some("Edition".into()))
            .map_err(|_| invalid_template("template validation failed"))?;
    PathTemplate::new(template)
        .and_then(|template| template.render(&context))
        .map(|_| ())
        .map_err(|_| invalid_template("must render to a safe relative output path"))
}

fn extract_id(path: Result<Path<i64>, PathRejection>) -> Result<IngestionRuleId, ApiError> {
    let Path(value) = path.map_err(|_| invalid_input("rule_id", "must be a positive integer"))?;
    IngestionRuleId::from_database(value).map_err(|_| not_found(value))
}

fn rule_envelope(rule: &IngestionRule) -> RuleEnvelope {
    RuleEnvelope {
        schema_version: SCHEMA_VERSION,
        rule: rule_dto(rule),
    }
}

fn rule_dto(rule: &IngestionRule) -> RuleDto {
    RuleDto {
        id: rule.id().get(),
        name: rule.name().to_owned(),
        source: directory_dto(rule.source()),
        destination: directory_dto(rule.destination()),
        media_kind_mode: rule.media_kind_mode(),
        placement: rule.placement(),
        path_template_override: rule.path_template_override().map(str::to_owned),
        enabled: rule.enabled(),
        status: rule_status(rule),
        last_error: rule.last_error().map(str::to_owned),
        created_at_ms: rule.created_at_ms(),
        updated_at_ms: rule.updated_at_ms(),
    }
}

fn directory_dto(directory: &RuleDirectory) -> RuleDirectoryDto {
    RuleDirectoryDto {
        root_id: directory.root_id().to_owned(),
        path: directory.relative_path().to_owned(),
    }
}

fn rule_status(rule: &IngestionRule) -> RuleStatus {
    if rule.last_error().is_some() {
        RuleStatus::Error
    } else if rule.enabled() {
        RuleStatus::Watching
    } else {
        RuleStatus::Paused
    }
}

fn map_json_rejection(_error: JsonRejection) -> ApiError {
    invalid_input(
        "body",
        "must be valid JSON matching the ingestion rule schema",
    )
}

fn map_runtime_error(error: IngestionRuntimeError) -> ApiError {
    match error {
        IngestionRuntimeError::Workspace(_) | IngestionRuntimeError::FilesystemPolicy(_) => {
            invalid_input(
                "directories",
                "must identify distinct, non-overlapping directories in configured roots",
            )
        }
        IngestionRuntimeError::Model(error) => map_model_error(&error),
        IngestionRuntimeError::Store(_) => ApiError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "ingestion_store_error",
            "Persistent ingestion rule operation failed",
            None,
        ),
    }
}

fn map_model_error(error: &IngestionModelError) -> ApiError {
    let field = match error {
        IngestionModelError::InvalidRequiredText { field, .. }
        | IngestionModelError::InvalidOptionalText { field, .. } => *field,
        _ => "body",
    };
    invalid_input(field, "must contain a supported bounded value")
}

fn invalid_template(reason: &str) -> ApiError {
    ApiError::new(
        StatusCode::UNPROCESSABLE_ENTITY,
        "invalid_template",
        "Ingestion rule template is invalid",
        Some(BTreeMap::from([(
            "path_template_override".to_owned(),
            reason.to_owned(),
        )])),
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

fn not_found(id: i64) -> ApiError {
    ApiError::new(
        StatusCode::NOT_FOUND,
        "ingestion_rule_not_found",
        "Ingestion rule not found",
        Some(BTreeMap::from([("rule_id".to_owned(), id.to_string())])),
    )
}
