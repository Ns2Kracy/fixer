use fixer_core::{
    Candidate, ExternalId, MatchEvidence, Matcher, MergeConflict, OutputOperation, OutputPlan,
    ResolutionWarning, Resolved, SourceRef,
};
use serde::Serialize;

use super::worker::JobFlowError;

const MAX_CANDIDATES: usize = 100;
const MAX_EVIDENCE_PER_CANDIDATE: usize = 32;
const MAX_WARNINGS: usize = 100;
const MAX_CONFLICTS: usize = 200;
const MAX_SOURCES_PER_CONFLICT: usize = 20;
const MAX_OPERATIONS: usize = 1_000;
const MAX_TEXT_CHARS: usize = 2_048;

#[derive(Debug, Serialize)]
pub(crate) struct ReviewArtifacts {
    pub candidates: Vec<CandidateArtifact>,
    pub candidates_truncated: bool,
    pub warnings: Vec<WarningArtifact>,
    pub warnings_truncated: bool,
    pub conflicts: Vec<ConflictArtifact>,
    pub conflicts_truncated: bool,
}

#[derive(Debug, Serialize)]
pub(crate) struct CandidateArtifact {
    pub index: u64,
    pub media_kind: fixer_core::MediaKind,
    pub provider: String,
    pub external_id: ExternalIdArtifact,
    pub title: String,
    pub year: Option<u16>,
    pub sequence: Option<String>,
    pub score: i32,
    pub evidence: Vec<EvidenceArtifact>,
    pub evidence_truncated: bool,
}

#[derive(Debug, Serialize)]
pub(crate) struct ExternalIdArtifact {
    pub namespace: String,
    pub value: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct EvidenceArtifact {
    pub kind: fixer_core::MatchEvidenceKind,
    pub points: i32,
    pub detail: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct WarningArtifact {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct ConflictArtifact {
    pub index: u64,
    pub field_path: String,
    pub message: String,
    pub providers: Vec<String>,
    pub providers_truncated: bool,
    pub sources: Vec<SourceArtifact>,
    pub sources_truncated: bool,
}

#[derive(Debug, Serialize)]
pub(crate) struct SourceArtifact {
    pub provider: String,
    pub external_id: Option<ExternalIdArtifact>,
    pub locale: Option<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct PlanArtifacts {
    pub output_root: String,
    pub operations: Vec<OperationArtifact>,
    pub operations_truncated: bool,
}

#[derive(Debug, Serialize)]
pub(crate) struct OperationArtifact {
    pub index: u64,
    pub kind: &'static str,
    pub source: Option<String>,
    pub target: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_bytes: Option<u64>,
}

pub(crate) fn candidates(
    query: &fixer_core::MatchQuery,
    values: &[Candidate],
) -> Result<(Vec<CandidateArtifact>, bool), JobFlowError> {
    let truncated = values.len() > MAX_CANDIDATES;
    let values = values
        .iter()
        .take(MAX_CANDIDATES)
        .enumerate()
        .map(|(index, candidate)| {
            let score = Matcher
                .score(query, candidate)
                .map_err(|error| JobFlowError::Matching(error.to_string()))?;
            let evidence_truncated = score.evidence.len() > MAX_EVIDENCE_PER_CANDIDATE;
            let evidence = score
                .evidence
                .iter()
                .take(MAX_EVIDENCE_PER_CANDIDATE)
                .map(evidence)
                .collect();
            let (title, year, sequence) = candidate_fields(candidate);
            Ok(CandidateArtifact {
                index: u64::try_from(index).map_err(|_| JobFlowError::IndexOverflow)?,
                media_kind: candidate.media_kind(),
                provider: candidate.provider().as_str().to_owned(),
                external_id: external_id(candidate.external_id()),
                title: text(title),
                year,
                sequence: sequence.map(text),
                score: score.total,
                evidence,
                evidence_truncated,
            })
        })
        .collect::<Result<Vec<_>, JobFlowError>>()?;
    Ok((values, truncated))
}

pub(crate) fn diagnostics<T>(resolved: &Resolved<T>) -> ReviewArtifacts {
    let warnings_truncated = resolved.warnings.len() > MAX_WARNINGS;
    let conflicts_truncated = resolved.conflicts.len() > MAX_CONFLICTS;
    ReviewArtifacts {
        candidates: Vec::new(),
        candidates_truncated: false,
        warnings: resolved
            .warnings
            .iter()
            .take(MAX_WARNINGS)
            .map(warning)
            .collect(),
        warnings_truncated,
        conflicts: resolved
            .conflicts
            .iter()
            .take(MAX_CONFLICTS)
            .enumerate()
            .map(|(index, conflict)| conflict_artifact(index, conflict, resolved))
            .collect(),
        conflicts_truncated,
    }
}

pub(crate) fn plan(plan: &OutputPlan) -> Result<PlanArtifacts, JobFlowError> {
    let operations_truncated = plan.operations().len() > MAX_OPERATIONS;
    let operations = plan
        .operations()
        .iter()
        .take(MAX_OPERATIONS)
        .enumerate()
        .map(|(index, operation)| operation_artifact(index, operation))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(PlanArtifacts {
        output_root: path(&plan.output_root),
        operations,
        operations_truncated,
    })
}

fn candidate_fields(candidate: &Candidate) -> (&str, Option<u16>, Option<&str>) {
    match candidate {
        Candidate::Movie(value) => (&value.title, value.year, value.sequence.as_deref()),
        Candidate::Television(value) => (&value.title, value.year, value.sequence.as_deref()),
        Candidate::Anime(value) => (&value.title, value.year, value.sequence.as_deref()),
        Candidate::Music(value) => (&value.title, value.year, value.sequence.as_deref()),
        Candidate::Book(value) => (&value.title, value.year, value.sequence.as_deref()),
    }
}

fn evidence(value: &MatchEvidence) -> EvidenceArtifact {
    EvidenceArtifact {
        kind: value.kind,
        points: value.points,
        detail: text(&value.detail),
    }
}

fn warning(value: &ResolutionWarning) -> WarningArtifact {
    WarningArtifact {
        code: text(&value.code),
        message: text(&value.message),
    }
}

fn conflict_artifact<T>(
    index: usize,
    value: &MergeConflict,
    resolved: &Resolved<T>,
) -> ConflictArtifact {
    let source_values = resolved.provenance.sources_for(&value.field_path);
    ConflictArtifact {
        index: u64::try_from(index).unwrap_or(u64::MAX),
        field_path: text(&value.field_path),
        message: text(&value.message),
        providers: value
            .providers
            .iter()
            .take(MAX_SOURCES_PER_CONFLICT)
            .map(|provider| provider.as_str().to_owned())
            .collect(),
        providers_truncated: value.providers.len() > MAX_SOURCES_PER_CONFLICT,
        sources: source_values
            .iter()
            .take(MAX_SOURCES_PER_CONFLICT)
            .map(source)
            .collect(),
        sources_truncated: source_values.len() > MAX_SOURCES_PER_CONFLICT,
    }
}

fn source(value: &SourceRef) -> SourceArtifact {
    SourceArtifact {
        provider: value.provider.as_str().to_owned(),
        external_id: value.external_id.as_ref().map(external_id),
        locale: value
            .locale
            .as_ref()
            .map(|locale| locale.as_str().to_owned()),
    }
}

fn external_id(value: &ExternalId) -> ExternalIdArtifact {
    ExternalIdArtifact {
        namespace: text(&value.namespace),
        value: text(&value.value),
    }
}

fn operation_artifact(
    index: usize,
    value: &OutputOperation,
) -> Result<OperationArtifact, JobFlowError> {
    let (kind, content_bytes) = match value {
        OutputOperation::CreateDirectory { .. } => ("create_directory", None),
        OutputOperation::WriteBytes { content, .. } => (
            "write",
            Some(u64::try_from(content.as_bytes().len()).map_err(|_| JobFlowError::CountOverflow)?),
        ),
        OutputOperation::Copy { .. } => ("copy", None),
        OutputOperation::Symlink { .. } => ("symlink", None),
        OutputOperation::Hardlink { .. } => ("hardlink", None),
        OutputOperation::Reflink { .. } => ("reflink", None),
    };
    Ok(OperationArtifact {
        index: u64::try_from(index).map_err(|_| JobFlowError::IndexOverflow)?,
        kind,
        source: value.source().map(path),
        target: value.target().map(path).unwrap_or_default(),
        content_bytes,
    })
}

fn text(value: &str) -> String {
    value.chars().take(MAX_TEXT_CHARS).collect()
}

fn path(value: &std::path::Path) -> String {
    text(&value.to_string_lossy())
}

#[cfg(test)]
mod tests {
    use std::time::UNIX_EPOCH;

    use fixer_core::{
        ExternalId, MergeConflict, OutputOperation, OutputPlan, PlannedContent, ProvenanceMap,
        ProviderId, Resolved, SourceRef,
    };

    use super::{diagnostics, plan};

    #[test]
    fn operation_previews_cover_every_kind_without_exposing_write_content() {
        let mut output = OutputPlan::new("/media/library");
        output.push(OutputOperation::create_directory("Movie").unwrap());
        output.push(
            OutputOperation::write_bytes("Movie/movie.json", PlannedContent::new(b"secret bytes"))
                .unwrap(),
        );
        output.push(OutputOperation::copy("source.mkv", "Movie/copy.mkv").unwrap());
        output.push(OutputOperation::symlink("source.mkv", "Movie/symlink.mkv").unwrap());
        output.push(OutputOperation::hardlink("source.mkv", "Movie/hardlink.mkv").unwrap());
        output.push(OutputOperation::reflink("source.mkv", "Movie/reflink.mkv").unwrap());

        let details = plan(&output).unwrap();
        assert_eq!(details.output_root, "/media/library");
        assert!(!details.operations_truncated);
        assert_eq!(
            details
                .operations
                .iter()
                .map(|operation| operation.kind)
                .collect::<Vec<_>>(),
            [
                "create_directory",
                "write",
                "copy",
                "symlink",
                "hardlink",
                "reflink"
            ]
        );
        assert_eq!(details.operations[1].content_bytes, Some(12));
        assert_eq!(details.operations[1].source, None);
        assert_eq!(details.operations[1].target, "Movie/movie.json");
        assert_eq!(
            serde_json::to_value(&details.operations[1]).unwrap(),
            serde_json::json!({
                "index": 1,
                "kind": "write",
                "source": null,
                "target": "Movie/movie.json",
                "content_bytes": 12
            })
        );
    }

    #[test]
    fn conflict_details_retain_provider_external_id_and_locale_visibility() {
        let provider = ProviderId::new("fixture.local").unwrap();
        let external_id = ExternalId::new("tmdb", "843").unwrap();
        let mut provenance = ProvenanceMap::new();
        provenance
            .add(
                "summaries",
                SourceRef::new(
                    provider.clone(),
                    Some(external_id),
                    Some("zh-CN".parse().unwrap()),
                    UNIX_EPOCH,
                ),
            )
            .unwrap();
        let resolved = Resolved {
            value: (),
            provenance,
            conflicts: vec![MergeConflict {
                field_path: "summaries".to_owned(),
                providers: vec![provider],
                message: "provider summaries differ".to_owned(),
            }],
            completeness: 1.0,
            warnings: Vec::new(),
        };

        let details = diagnostics(&resolved);
        assert_eq!(details.conflicts.len(), 1);
        assert_eq!(details.conflicts[0].providers, ["fixture.local"]);
        assert_eq!(details.conflicts[0].sources.len(), 1);
        assert_eq!(details.conflicts[0].sources[0].provider, "fixture.local");
        assert_eq!(
            details.conflicts[0].sources[0].locale.as_deref(),
            Some("zh-CN")
        );
        let external_id = details.conflicts[0].sources[0]
            .external_id
            .as_ref()
            .unwrap();
        assert_eq!(external_id.namespace, "tmdb");
        assert_eq!(external_id.value, "843");
    }
}
