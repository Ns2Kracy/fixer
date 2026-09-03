//! Deterministic JSON movie writer.

use fixer_core::{
    MetadataDocument, Movie, OutputOperation, OutputPlan, PlannedContent, PlanningError, Resolved,
    WriteRequest, Writer,
};
use std::path::Path;

/// Built-in deterministic movie JSON writer.
#[derive(Debug, Clone, Default)]
pub struct JsonWriter;
impl JsonWriter {
    /// Plans `movie.json` for a resolved movie without touching the filesystem.
    pub fn plan_resolved(
        &self,
        resolved: &Resolved<Movie>,
        output_root: impl AsRef<Path>,
    ) -> Result<OutputPlan, PlanningError> {
        Self::plan_movie(&resolved.value, output_root.as_ref())
    }
    fn plan_movie(movie: &Movie, output_root: &Path) -> Result<OutputPlan, PlanningError> {
        let mut bytes = serde_json::to_vec_pretty(movie)?;
        bytes.push(b'\n');
        let mut plan = OutputPlan::new(output_root);
        plan.push(OutputOperation::write_bytes(
            "movie.json",
            PlannedContent::new(bytes),
        )?);
        Ok(plan)
    }
}
impl Writer for JsonWriter {
    fn plan(&self, request: WriteRequest) -> Result<OutputPlan, PlanningError> {
        match request.document {
            MetadataDocument::Movie(movie) => Self::plan_movie(&movie, &request.output_root),
            _ => Err(PlanningError::UnsupportedDocument),
        }
    }
}
