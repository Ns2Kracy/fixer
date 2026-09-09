use std::{fmt, str::FromStr};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::ingestion::model::RulePlacement;

const SCHEMA_VERSION: u8 = 1;

const fn automatic_selection() -> fixer_core::ScrapeSelection {
    fixer_core::ScrapeSelection::Automatic
}

const fn is_automatic_selection(selection: &fixer_core::ScrapeSelection) -> bool {
    matches!(selection, fixer_core::ScrapeSelection::Automatic)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SchemaVersion;

impl Serialize for SchemaVersion {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_u8(SCHEMA_VERSION)
    }
}

impl<'de> Deserialize<'de> for SchemaVersion {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let version = u8::deserialize(deserializer)?;
        if version == SCHEMA_VERSION {
            Ok(Self)
        } else {
            Err(serde::de::Error::custom(format_args!(
                "unsupported job schema version {version}; expected {SCHEMA_VERSION}"
            )))
        }
    }
}

/// Media kind accepted by the persistent job API.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobMediaKind {
    Anime,
    Book,
    Movie,
    Music,
    Television,
}

/// Immutable organization settings captured when a job is created.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JobOrganizationDto {
    pub destination_path: String,
    pub placement: RulePlacement,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path_template: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin_rule_id: Option<i64>,
    pub auto_execute: bool,
}

/// Versioned, server-owned input persisted for one scraping job.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JobInputDto {
    schema_version: SchemaVersion,
    media_kind: JobMediaKind,
    input_path: String,
    apply: bool,
    #[serde(
        default = "automatic_selection",
        skip_serializing_if = "is_automatic_selection"
    )]
    selection: fixer_core::ScrapeSelection,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    unattended: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    correction_of: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    retry_of: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    organization: Option<JobOrganizationDto>,
}

impl JobInputDto {
    pub fn new(media_kind: JobMediaKind, input_path: impl Into<String>, apply: bool) -> Self {
        Self {
            schema_version: SchemaVersion,
            media_kind,
            input_path: input_path.into(),
            apply,
            selection: fixer_core::ScrapeSelection::Automatic,
            unattended: false,
            correction_of: None,
            retry_of: None,
            organization: None,
        }
    }

    pub fn with_organization(mut self, organization: JobOrganizationDto) -> Self {
        self.organization = Some(organization);
        self
    }

    pub const fn media_kind(&self) -> JobMediaKind {
        self.media_kind
    }

    pub fn input_path(&self) -> &str {
        &self.input_path
    }

    pub const fn apply(&self) -> bool {
        self.apply
    }

    pub fn with_selection(mut self, selection: fixer_core::ScrapeSelection) -> Self {
        self.selection = selection;
        self
    }

    pub const fn selection(&self) -> &fixer_core::ScrapeSelection {
        &self.selection
    }

    pub const fn unattended(&self) -> bool {
        self.unattended
    }

    pub const fn with_unattended(mut self) -> Self {
        self.unattended = true;
        self
    }

    pub const fn correction_of(&self) -> Option<i64> {
        self.correction_of
    }

    pub const fn with_correction_of(mut self, run_id: i64) -> Self {
        self.correction_of = Some(run_id);
        self
    }

    pub const fn retry_of(&self) -> Option<i64> {
        self.retry_of
    }

    pub const fn with_retry_of(mut self, run_id: i64) -> Self {
        self.retry_of = Some(run_id);
        self
    }

    pub const fn organization(&self) -> Option<&JobOrganizationDto> {
        self.organization.as_ref()
    }
}

/// Versioned bounded progress persisted between worker stages.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProgressSummary {
    schema_version: SchemaVersion,
    stage: String,
    completed_items: u64,
    total_items: Option<u64>,
}

impl ProgressSummary {
    pub fn new(stage: impl Into<String>, completed_items: u64, total_items: Option<u64>) -> Self {
        Self {
            schema_version: SchemaVersion,
            stage: stage.into(),
            completed_items,
            total_items,
        }
    }
}

/// Bounded reason automatic execution deferred to manual review.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AutoReviewReason {
    ManualJob,
    CandidateListTruncated,
    NoCandidates,
    MetadataConflicts,
    ProviderEnrichmentFailed,
    DiagnosticsTruncated,
    InvalidPlan,
    DestinationCollision,
}

/// Versioned candidate/conflict counts persisted for review.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewSummary {
    schema_version: SchemaVersion,
    candidate_count: u64,
    conflict_count: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    automation_reason: Option<AutoReviewReason>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    selected_target: Option<fixer_core::ProviderTarget>,
}

impl ReviewSummary {
    pub const fn new(candidate_count: u64, conflict_count: u64) -> Self {
        Self {
            schema_version: SchemaVersion,
            candidate_count,
            conflict_count,
            automation_reason: None,
            selected_target: None,
        }
    }

    pub const fn with_automation_reason(mut self, reason: AutoReviewReason) -> Self {
        self.automation_reason = Some(reason);
        self
    }

    pub const fn candidate_count(&self) -> u64 {
        self.candidate_count
    }

    pub const fn conflict_count(&self) -> u64 {
        self.conflict_count
    }

    pub fn with_selected_target(mut self, target: fixer_core::ProviderTarget) -> Self {
        self.selected_target = Some(target);
        self
    }

    pub const fn selected_target(&self) -> Option<&fixer_core::ProviderTarget> {
        self.selected_target.as_ref()
    }

    pub const fn automation_reason(&self) -> Option<AutoReviewReason> {
        self.automation_reason
    }
}

/// Versioned, bounded review choices persisted without resolved Core snapshots.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewDecisionDto {
    schema_version: SchemaVersion,
    candidate_index: u64,
    accepted_conflict_indexes: Vec<u64>,
}

impl ReviewDecisionDto {
    pub const fn new(candidate_index: u64, accepted_conflict_indexes: Vec<u64>) -> Self {
        Self {
            schema_version: SchemaVersion,
            candidate_index,
            accepted_conflict_indexes,
        }
    }

    pub const fn candidate_index(&self) -> u64 {
        self.candidate_index
    }

    pub fn accepted_conflict_indexes(&self) -> &[u64] {
        &self.accepted_conflict_indexes
    }
}

/// Versioned output-plan counts persisted without operation bytes or Core snapshots.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanSummary {
    schema_version: SchemaVersion,
    operation_count: u64,
    requires_confirmation: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    fingerprint: Option<String>,
}

impl PlanSummary {
    pub const fn new(operation_count: u64, requires_confirmation: bool) -> Self {
        Self {
            schema_version: SchemaVersion,
            operation_count,
            requires_confirmation,
            fingerprint: None,
        }
    }

    pub fn with_fingerprint(mut self, fingerprint: String) -> Self {
        self.fingerprint = Some(fingerprint);
        self
    }

    pub fn fingerprint(&self) -> Option<&str> {
        self.fingerprint.as_deref()
    }

    pub const fn operation_count(&self) -> u64 {
        self.operation_count
    }

    pub const fn requires_confirmation(&self) -> bool {
        self.requires_confirmation
    }
}

/// Bounded, path-safe detail for one failed output operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionFailureSummary {
    schema_version: SchemaVersion,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    operation_index: Option<u64>,
    code: String,
    message: String,
}

impl ExecutionFailureSummary {
    pub fn new(
        operation_index: Option<u64>,
        code: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            schema_version: SchemaVersion,
            operation_index,
            code: code.into(),
            message: message.into(),
        }
    }
}

/// Versioned execution counts and optional failure detail persisted without filesystem payloads.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionSummary {
    schema_version: SchemaVersion,
    completed_operations: u64,
    failed_operations: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    failure: Option<ExecutionFailureSummary>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    operations: Vec<fixer_core::OperationReport>,
}

impl ExecutionSummary {
    pub const fn new(completed_operations: u64, failed_operations: u64) -> Self {
        Self {
            schema_version: SchemaVersion,
            completed_operations,
            failed_operations,
            failure: None,
            operations: Vec::new(),
        }
    }

    pub fn with_failure(mut self, failure: ExecutionFailureSummary) -> Self {
        self.failure = Some(failure);
        self
    }

    pub fn with_operations(mut self, operations: Vec<fixer_core::OperationReport>) -> Self {
        self.operations = operations;
        self
    }

    pub fn without_operations(mut self) -> Self {
        self.operations.clear();
        self
    }

    pub fn operations(&self) -> &[fixer_core::OperationReport] {
        &self.operations
    }

    pub const fn completed_operations(&self) -> u64 {
        self.completed_operations
    }
}

/// Persistent worker lifecycle state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobState {
    Queued,
    Scanning,
    Searching,
    Resolving,
    AwaitingConfirmation,
    Planning,
    Writing,
    Completed,
    Failed,
    Cancelled,
    Interrupted,
}

impl JobState {
    pub const ALL: [Self; 11] = [
        Self::Queued,
        Self::Scanning,
        Self::Searching,
        Self::Resolving,
        Self::AwaitingConfirmation,
        Self::Planning,
        Self::Writing,
        Self::Completed,
        Self::Failed,
        Self::Cancelled,
        Self::Interrupted,
    ];

    pub const fn can_transition_to(self, next: Self) -> bool {
        use JobState::{
            AwaitingConfirmation, Cancelled, Completed, Failed, Interrupted, Planning, Queued,
            Resolving, Scanning, Searching, Writing,
        };

        matches!(
            (self, next),
            (Queued, Scanning | Cancelled)
                | (
                    Scanning,
                    Queued | Searching | Failed | Cancelled | Interrupted
                )
                | (Searching, Resolving | Failed | Cancelled | Interrupted)
                | (
                    Resolving,
                    AwaitingConfirmation | Failed | Cancelled | Interrupted
                )
                | (AwaitingConfirmation, Planning | Failed | Cancelled)
                | (Planning, Writing | Failed | Cancelled | Interrupted)
                | (Writing, Completed | Failed | Interrupted)
                | (Interrupted, Queued)
        )
    }
}

impl fmt::Display for JobState {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Queued => "queued",
            Self::Scanning => "scanning",
            Self::Searching => "searching",
            Self::Resolving => "resolving",
            Self::AwaitingConfirmation => "awaiting_confirmation",
            Self::Planning => "planning",
            Self::Writing => "writing",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::Interrupted => "interrupted",
        })
    }
}

impl FromStr for JobState {
    type Err = JobStateParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "queued" => Ok(Self::Queued),
            "scanning" => Ok(Self::Scanning),
            "searching" => Ok(Self::Searching),
            "resolving" => Ok(Self::Resolving),
            "awaiting_confirmation" => Ok(Self::AwaitingConfirmation),
            "planning" => Ok(Self::Planning),
            "writing" => Ok(Self::Writing),
            "completed" => Ok(Self::Completed),
            "failed" => Ok(Self::Failed),
            "cancelled" => Ok(Self::Cancelled),
            "interrupted" => Ok(Self::Interrupted),
            _ => Err(JobStateParseError(value.to_owned())),
        }
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
#[error("unknown persistent job state `{0}`")]
pub struct JobStateParseError(String);
