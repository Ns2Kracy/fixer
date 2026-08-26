mod sqlite;

use std::{num::NonZeroI64, path::PathBuf};

use thiserror::Error;

use crate::jobs::model::{
    ExecutionSummary, JobInputDto, JobState, PlanSummary, ProgressSummary, ReviewDecisionDto,
    ReviewSummary,
};

pub use sqlite::SqliteJobStore;

/// Stable database identity for a persisted job.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct JobId(NonZeroI64);

impl JobId {
    pub const fn get(self) -> i64 {
        self.0.get()
    }

    pub(crate) fn from_database(value: i64) -> Result<Self, StoreError> {
        if value <= 0 {
            return Err(StoreError::CorruptRecord(
                "job id must be positive".to_owned(),
            ));
        }
        Ok(Self(
            NonZeroI64::new(value).expect("positive values are non-zero"),
        ))
    }
}

/// Fully decoded persistent job record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobRecord {
    id: JobId,
    input: JobInputDto,
    state: JobState,
    progress: Option<ProgressSummary>,
    review: Option<ReviewSummary>,
    review_decision: Option<ReviewDecisionDto>,
    plan: Option<PlanSummary>,
    execution: Option<ExecutionSummary>,
    created_at_ms: i64,
    updated_at_ms: i64,
}

impl JobRecord {
    pub const fn id(&self) -> JobId {
        self.id
    }

    pub const fn input(&self) -> &JobInputDto {
        &self.input
    }

    pub const fn state(&self) -> JobState {
        self.state
    }

    pub const fn progress(&self) -> Option<&ProgressSummary> {
        self.progress.as_ref()
    }

    pub const fn review(&self) -> Option<&ReviewSummary> {
        self.review.as_ref()
    }

    pub const fn review_decision(&self) -> Option<&ReviewDecisionDto> {
        self.review_decision.as_ref()
    }

    pub const fn plan(&self) -> Option<&PlanSummary> {
        self.plan.as_ref()
    }

    pub const fn execution(&self) -> Option<&ExecutionSummary> {
        self.execution.as_ref()
    }

    pub const fn created_at_ms(&self) -> i64 {
        self.created_at_ms
    }

    pub const fn updated_at_ms(&self) -> i64 {
        self.updated_at_ms
    }

    pub(crate) fn from_parts(parts: JobRecordParts) -> Self {
        Self {
            id: parts.id,
            input: parts.input,
            state: parts.state,
            progress: parts.progress,
            review: parts.review,
            review_decision: parts.review_decision,
            plan: parts.plan,
            execution: parts.execution,
            created_at_ms: parts.created_at_ms,
            updated_at_ms: parts.updated_at_ms,
        }
    }
}

pub(crate) struct JobRecordParts {
    pub id: JobId,
    pub input: JobInputDto,
    pub state: JobState,
    pub progress: Option<ProgressSummary>,
    pub review: Option<ReviewSummary>,
    pub review_decision: Option<ReviewDecisionDto>,
    pub plan: Option<PlanSummary>,
    pub execution: Option<ExecutionSummary>,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

/// Summary fields to persist atomically with a state transition.
#[derive(Debug, Clone, Default)]
pub struct JobUpdate {
    pub(crate) progress: Option<ProgressSummary>,
    pub(crate) review: Option<ReviewSummary>,
    pub(crate) review_decision: Option<ReviewDecisionDto>,
    pub(crate) plan: Option<PlanSummary>,
    pub(crate) execution: Option<ExecutionSummary>,
}

impl JobUpdate {
    pub fn with_progress(mut self, progress: ProgressSummary) -> Self {
        self.progress = Some(progress);
        self
    }

    pub fn with_review(mut self, review: ReviewSummary) -> Self {
        self.review = Some(review);
        self
    }

    pub fn with_review_decision(mut self, decision: ReviewDecisionDto) -> Self {
        self.review_decision = Some(decision);
        self
    }

    pub fn with_plan(mut self, plan: PlanSummary) -> Self {
        self.plan = Some(plan);
        self
    }

    pub fn with_execution(mut self, execution: ExecutionSummary) -> Self {
        self.execution = Some(execution);
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecutionReservation {
    Reserved(JobRecord),
    Existing(JobRecord),
}

impl ExecutionReservation {
    pub const fn job(&self) -> &JobRecord {
        match self {
            Self::Reserved(job) | Self::Existing(job) => job,
        }
    }
}

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("invalid job state transition from {from} to {to}")]
    InvalidTransition { from: JobState, to: JobState },
    #[error("job {id} does not exist")]
    NotFound { id: i64 },
    #[error("job {id} state changed: expected {expected}, found {actual}")]
    StateConflict {
        id: i64,
        expected: JobState,
        actual: JobState,
    },
    #[error("job {id} must reserve an idempotency key before writing")]
    ExecutionReservationRequired { id: i64 },
    #[error("job {id} has a reserved execution and cannot be retried automatically")]
    ReservedExecutionRetry { id: i64 },
    #[error("job {id} already has a different execution idempotency request")]
    IdempotencyConflict { id: i64 },
    #[error("persistent job record is invalid: {0}")]
    CorruptRecord(String),
    #[error("system clock is before the Unix epoch")]
    ClockBeforeEpoch,
    #[error("system timestamp exceeds SQLite integer range")]
    TimestampOverflow,
    #[error("job database `{path}` is already open by another store process")]
    AlreadyOpen { path: PathBuf },
    #[error("job store filesystem operation failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("SQLite migration failed: {0}")]
    Migration(#[from] sqlx::migrate::MigrateError),
    #[error("SQLite job store failed: {0}")]
    Database(#[from] sqlx::Error),
    #[error("persistent job JSON is invalid: {0}")]
    Json(#[from] serde_json::Error),
}
