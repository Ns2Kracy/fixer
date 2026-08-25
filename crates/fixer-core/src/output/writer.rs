//! Metadata writer planning contract.

use crate::{BoxFuture, MetadataDocument, OutputPlan};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use thiserror::Error;

/// Input supplied to a metadata writer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WriteRequest {
    pub document: MetadataDocument,
    pub output_root: PathBuf,
}
impl WriteRequest {
    /// Constructs a writer request.
    pub const fn new(document: MetadataDocument, output_root: PathBuf) -> Self {
        Self {
            document,
            output_root,
        }
    }
}

/// Structured output planning failures.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[non_exhaustive]
pub enum PlanningError {
    #[error("writer does not support this metadata document")]
    UnsupportedDocument,
    #[error("invalid output plan: {0}")]
    InvalidPlan(String),
    #[error("metadata serialization failed: {0}")]
    Serialization(String),
}
impl From<serde_json::Error> for PlanningError {
    fn from(error: serde_json::Error) -> Self {
        Self::Serialization(error.to_string())
    }
}
impl From<crate::CoreError> for PlanningError {
    fn from(error: crate::CoreError) -> Self {
        Self::InvalidPlan(error.to_string())
    }
}

/// Runtime-neutral metadata writer contract.
pub trait Writer: Send + Sync {
    fn plan(&self, request: WriteRequest) -> Result<OutputPlan, PlanningError>;
}

// Keep BoxFuture reachable here for writer extensions that add async preparation
// outside the core Writer contract without introducing a runtime dependency.
#[allow(dead_code)]
type RuntimeNeutralFuture<'a, T> = BoxFuture<'a, T>;
