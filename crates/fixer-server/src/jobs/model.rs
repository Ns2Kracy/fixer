use std::{fmt, str::FromStr};

use serde::{Deserialize, Serialize};
use thiserror::Error;

const SCHEMA_VERSION: u8 = 1;

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

/// Versioned, server-owned input persisted for one scraping job.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JobInputDto {
    schema_version: SchemaVersion,
    media_kind: JobMediaKind,
    input_path: String,
    apply: bool,
}

impl JobInputDto {
    pub fn new(media_kind: JobMediaKind, input_path: impl Into<String>, apply: bool) -> Self {
        Self {
            schema_version: SchemaVersion,
            media_kind,
            input_path: input_path.into(),
            apply,
        }
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

/// Versioned candidate/conflict counts persisted for review.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewSummary {
    schema_version: SchemaVersion,
    candidate_count: u64,
    conflict_count: u64,
}

impl ReviewSummary {
    pub const fn new(candidate_count: u64, conflict_count: u64) -> Self {
        Self {
            schema_version: SchemaVersion,
            candidate_count,
            conflict_count,
        }
    }
}

/// Versioned output-plan counts persisted without operation bytes or Core snapshots.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanSummary {
    schema_version: SchemaVersion,
    operation_count: u64,
    requires_confirmation: bool,
}

impl PlanSummary {
    pub const fn new(operation_count: u64, requires_confirmation: bool) -> Self {
        Self {
            schema_version: SchemaVersion,
            operation_count,
            requires_confirmation,
        }
    }
}

/// Versioned execution counts persisted without filesystem payloads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionSummary {
    schema_version: SchemaVersion,
    completed_operations: u64,
    failed_operations: u64,
}

impl ExecutionSummary {
    pub const fn new(completed_operations: u64, failed_operations: u64) -> Self {
        Self {
            schema_version: SchemaVersion,
            completed_operations,
            failed_operations,
        }
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
                | (Scanning, Searching | Failed | Cancelled | Interrupted)
                | (Searching, Resolving | Failed | Cancelled | Interrupted)
                | (
                    Resolving,
                    AwaitingConfirmation | Failed | Cancelled | Interrupted
                )
                | (AwaitingConfirmation, Planning | Failed | Cancelled)
                | (Planning, Writing | Failed | Cancelled | Interrupted)
                | (Writing, Completed | Failed | Cancelled | Interrupted)
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
