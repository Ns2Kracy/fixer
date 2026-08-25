//! Provenance manifest writer.

use fixer_core::{Movie, OutputOperation, OutputPlan, PlannedContent, PlanningError, Resolved};
use serde::Serialize;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

/// Writer for a deterministic provenance and planned-files manifest.
#[derive(Debug, Clone)]
pub struct ManifestWriter {
    planned_files: BTreeMap<String, PathBuf>,
}
impl ManifestWriter {
    /// Constructs a manifest writer from named planned files.
    pub const fn new(planned_files: BTreeMap<String, PathBuf>) -> Self {
        Self { planned_files }
    }
    /// Plans `fixer-manifest.json` without touching the filesystem.
    pub fn plan_resolved(
        &self,
        resolved: &Resolved<Movie>,
        output_root: impl AsRef<Path>,
    ) -> Result<OutputPlan, PlanningError> {
        #[derive(Serialize)]
        struct Manifest<'a> {
            schema_version: u8,
            work_id: &'a str,
            provenance: &'a fixer_core::ProvenanceMap,
            planned_files: &'a BTreeMap<String, PathBuf>,
        }
        let manifest = Manifest {
            schema_version: 1,
            work_id: resolved.value.id.as_str(),
            provenance: &resolved.provenance,
            planned_files: &self.planned_files,
        };
        let mut bytes = serde_json::to_vec_pretty(&manifest)?;
        bytes.push(b'\n');
        let mut plan = OutputPlan::new(output_root.as_ref().to_path_buf());
        plan.push(OutputOperation::write_bytes(
            "fixer-manifest.json",
            PlannedContent::new(bytes),
        )?);
        Ok(plan)
    }
}
