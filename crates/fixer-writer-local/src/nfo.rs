//! Deterministic Kodi/Jellyfin-compatible movie NFO writer.

use crate::television;
use fixer_core::{
    MetadataDocument, Movie, OutputOperation, OutputPlan, PlannedContent, PlanningError, Resolved,
    WriteRequest, Writer,
};
use std::path::Path;

/// Built-in deterministic movie NFO writer.
#[derive(Debug, Clone, Default)]
pub struct NfoWriter;
impl NfoWriter {
    /// Plans `movie.nfo` for a resolved movie without touching the filesystem.
    pub fn plan_resolved(
        &self,
        resolved: &Resolved<Movie>,
        output_root: impl AsRef<Path>,
    ) -> Result<OutputPlan, PlanningError> {
        self.plan_movie(&resolved.value, output_root.as_ref())
    }
    fn plan_movie(&self, movie: &Movie, output_root: &Path) -> Result<OutputPlan, PlanningError> {
        let title = movie
            .titles
            .entries()
            .first()
            .map(|entry| entry.value().as_str())
            .unwrap_or_default();
        let original = movie
            .titles
            .entries()
            .iter()
            .find(|entry| {
                entry
                    .language()
                    .is_some_and(|language| language.primary_language() == "en")
            })
            .map(|entry| entry.value().as_str());
        let summary = movie
            .summaries
            .entries()
            .first()
            .map(|entry| entry.value().as_str());
        let mut xml = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<movie>\n");
        element(&mut xml, "title", title);
        if let Some(original) = original {
            element(&mut xml, "originaltitle", original);
        }
        if let Some(year) = movie.release_year() {
            element(&mut xml, "year", &year.to_string());
        }
        if let Some(summary) = summary {
            element(&mut xml, "plot", summary);
        }
        xml.push_str("</movie>\n");
        let mut plan = OutputPlan::new(output_root);
        plan.push(OutputOperation::write_bytes(
            "movie.nfo",
            PlannedContent::new(xml.into_bytes()),
        )?);
        Ok(plan)
    }
}
impl Writer for NfoWriter {
    fn plan(&self, request: WriteRequest) -> Result<OutputPlan, PlanningError> {
        match request.document {
            MetadataDocument::Movie(movie) => self.plan_movie(&movie, &request.output_root),
            MetadataDocument::Television(series) => {
                television::plan_series(&series, &request.output_root)
            }
            _ => Err(PlanningError::UnsupportedDocument),
        }
    }
}
pub(crate) fn element(output: &mut String, name: &str, value: &str) {
    output.push_str("  <");
    output.push_str(name);
    output.push('>');
    output.push_str(&escape_xml(value));
    output.push_str("</");
    output.push_str(name);
    output.push_str(">\n");
}
pub(crate) fn escape_xml(value: &str) -> String {
    value
        .chars()
        .flat_map(|character| match character {
            '&' => "&amp;".chars().collect::<Vec<_>>(),
            '<' => "&lt;".chars().collect(),
            '>' => "&gt;".chars().collect(),
            '\"' => "&quot;".chars().collect(),
            '\'' => "&apos;".chars().collect(),
            value => vec![value],
        })
        .collect()
}
