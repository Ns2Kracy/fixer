//! Resolved values with field provenance and merge diagnostics.

use crate::{ProvenanceMap, ProviderId};
use serde::{Deserialize, Serialize};

/// A disagreement retained during merge rather than silently discarded.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MergeConflict {
    pub field_path: String,
    pub providers: Vec<ProviderId>,
    pub message: String,
}

/// A non-fatal resolution warning.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolutionWarning {
    pub code: String,
    pub message: String,
}

/// A resolved domain value and its diagnostics.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Resolved<T> {
    pub value: T,
    pub provenance: ProvenanceMap,
    pub conflicts: Vec<MergeConflict>,
    pub completeness: f32,
    pub warnings: Vec<ResolutionWarning>,
}

impl<T> Resolved<T> {
    /// Borrows the resolved value.
    pub const fn value(&self) -> &T {
        &self.value
    }
}
