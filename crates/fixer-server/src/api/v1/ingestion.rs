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
            IngestionModelError, IngestionRule, IngestionRuleId, IngestionRuleInput,
            IngestionSourceId, MediaKindMode, RuleDirectory, RulePlacement, RuleStatus,
        },
    },
    jobs::model::JobMediaKind,
    store::StoreError,
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
        .route(
            "/ingestion-rules/{id}/reviews",
            get(list_reviews).fallback(crate::api::error::method_not_allowed),
        )
        .route(
            "/ingestion-sources/{id}/resolve",
            axum::routing::post(resolve_review).fallback(crate::api::error::method_not_allowed),
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
    review_count: u64,
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

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ResolveReviewRequest {
    media_kind: JobMediaKind,
}

#[derive(Serialize)]
struct SourceReviewDto {
    source_id: i64,
    relative_path: String,
    media_kinds: Vec<JobMediaKind>,
}

#[derive(Serialize)]
struct SourceReviewListEnvelope {
    schema_version: u32,
    rule_id: i64,
    reviews: Vec<SourceReviewDto>,
}

#[derive(Serialize)]
struct ResolveReviewEnvelope {
    schema_version: u32,
    source_id: i64,
    job_id: i64,
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
    let stored = runtime
        .list_rules()
        .await
        .map_err(|error| map_runtime_error(&error))?;
    let mut rules = Vec::with_capacity(stored.len());
    for rule in stored {
        let status = runtime
            .rule_status(&rule)
            .await
            .map_err(|error| map_runtime_error(&error))?;
        let review_count = runtime
            .review_count(rule.id())
            .await
            .map_err(|error| map_runtime_error(&error))?;
        rules.push(rule_dto(&rule, status, review_count));
    }
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
        .map_err(|error| map_runtime_error(&error))?;
    let status = runtime
        .rule_status(&rule)
        .await
        .map_err(|error| map_runtime_error(&error))?;
    let review_count = runtime
        .review_count(rule.id())
        .await
        .map_err(|error| map_runtime_error(&error))?;
    Ok((
        StatusCode::CREATED,
        Json(rule_envelope(&rule, status, review_count)),
    ))
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
        .map_err(|error| map_runtime_error(&error))?
        .ok_or_else(|| not_found(id.get()))?;
    let status = runtime
        .rule_status(&rule)
        .await
        .map_err(|error| map_runtime_error(&error))?;
    let review_count = runtime
        .review_count(rule.id())
        .await
        .map_err(|error| map_runtime_error(&error))?;
    Ok(Json(rule_envelope(&rule, status, review_count)))
}

async fn delete_rule(
    State(runtime): State<IngestionRuntime>,
    path: Result<Path<i64>, PathRejection>,
) -> Result<StatusCode, ApiError> {
    let id = extract_id(path)?;
    if !runtime
        .delete_rule(id)
        .await
        .map_err(|error| map_runtime_error(&error))?
    {
        return Err(not_found(id.get()));
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn list_reviews(
    State(runtime): State<IngestionRuntime>,
    path: Result<Path<i64>, PathRejection>,
) -> Result<Json<SourceReviewListEnvelope>, ApiError> {
    let id = extract_id(path)?;
    let reviews = runtime
        .list_reviews(id)
        .await
        .map_err(|error| map_runtime_error(&error))?
        .ok_or_else(|| not_found(id.get()))?;
    Ok(Json(SourceReviewListEnvelope {
        schema_version: SCHEMA_VERSION,
        rule_id: id.get(),
        reviews: reviews
            .into_iter()
            .map(|review| SourceReviewDto {
                source_id: review.source_id().get(),
                relative_path: review.relative_source_path().to_owned(),
                media_kinds: review.media_kinds().to_vec(),
            })
            .collect(),
    }))
}

async fn resolve_review(
    State(runtime): State<IngestionRuntime>,
    path: Result<Path<i64>, PathRejection>,
    request: Result<Json<ResolveReviewRequest>, JsonRejection>,
) -> Result<(StatusCode, Json<ResolveReviewEnvelope>), ApiError> {
    let source_id = extract_source_id(path)?;
    let Json(request) = request.map_err(map_json_rejection)?;
    let job = runtime
        .resolve_review(source_id, request.media_kind)
        .await
        .map_err(|error| map_runtime_error(&error))?
        .ok_or_else(|| source_not_found(source_id.get()))?;
    Ok((
        StatusCode::ACCEPTED,
        Json(ResolveReviewEnvelope {
            schema_version: SCHEMA_VERSION,
            source_id: source_id.get(),
            job_id: job.id().get(),
        }),
    ))
}

async fn scan_rule(
    State(runtime): State<IngestionRuntime>,
    path: Result<Path<i64>, PathRejection>,
) -> Result<(StatusCode, Json<ScanEnvelope>), ApiError> {
    let id = extract_id(path)?;
    if !runtime
        .request_rescan(id)
        .await
        .map_err(|error| map_runtime_error(&error))?
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
        .map_err(|error| map_runtime_error(&error))?;
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

fn extract_source_id(
    path: Result<Path<i64>, PathRejection>,
) -> Result<IngestionSourceId, ApiError> {
    let Path(value) = path.map_err(|_| invalid_input("source_id", "must be a positive integer"))?;
    IngestionSourceId::from_database(value).map_err(|_| source_not_found(value))
}

fn rule_envelope(rule: &IngestionRule, status: RuleStatus, review_count: u64) -> RuleEnvelope {
    RuleEnvelope {
        schema_version: SCHEMA_VERSION,
        rule: rule_dto(rule, status, review_count),
    }
}

fn rule_dto(rule: &IngestionRule, status: RuleStatus, review_count: u64) -> RuleDto {
    RuleDto {
        id: rule.id().get(),
        name: rule.name().to_owned(),
        source: directory_dto(rule.source()),
        destination: directory_dto(rule.destination()),
        media_kind_mode: rule.media_kind_mode(),
        placement: rule.placement(),
        path_template_override: rule.path_template_override().map(str::to_owned),
        enabled: rule.enabled(),
        status,
        review_count,
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

fn map_json_rejection(_error: JsonRejection) -> ApiError {
    invalid_input(
        "body",
        "must be valid JSON matching the ingestion rule schema",
    )
}

fn map_runtime_error(error: &IngestionRuntimeError) -> ApiError {
    match error {
        IngestionRuntimeError::Workspace(_) | IngestionRuntimeError::FilesystemPolicy(_) => {
            invalid_input(
                "directories",
                "must identify distinct, non-overlapping directories in configured roots",
            )
        }
        IngestionRuntimeError::Model(error) => map_model_error(error),
        IngestionRuntimeError::InvalidReviewMediaKind => {
            invalid_input("media_kind", "must be one of the offered review choices")
        }
        IngestionRuntimeError::ReviewRuleMissing => ApiError::new(
            StatusCode::CONFLICT,
            "review_rule_missing",
            "The folder rule for this review no longer exists",
            None,
        ),
        IngestionRuntimeError::Jobs => ApiError::new(
            StatusCode::CONFLICT,
            "review_job_error",
            "The reviewed item could not be queued; rescan the folder and retry",
            None,
        ),
        IngestionRuntimeError::Store(StoreError::IngestionRuleLimit { limit }) => ApiError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "ingestion_rule_limit",
            "Folder rule limit reached",
            Some(BTreeMap::from([(
                "rules".to_owned(),
                format!("delete an existing rule before adding another (limit: {limit})"),
            )])),
        ),
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

fn source_not_found(id: i64) -> ApiError {
    ApiError::new(
        StatusCode::NOT_FOUND,
        "ingestion_source_not_found",
        "Folder review item was not found",
        Some(BTreeMap::from([("source_id".to_owned(), id.to_string())])),
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
