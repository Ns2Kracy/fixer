use fixer_core::{OutputOperation, OutputPlan};
use fixer_provider_local::ScanWarning;
use serde::Serialize;
use std::path::Path;

pub(crate) const SCHEMA_VERSION: u8 = 1;

#[derive(Debug, Serialize)]
pub(crate) struct ScanDto {
    schema_version: u8,
    kind: &'static str,
    root: String,
    documents: usize,
    warnings: Vec<ScanWarningDto>,
}

impl ScanDto {
    pub(crate) fn new(
        kind: &'static str,
        root: &Path,
        documents: usize,
        warnings: &[ScanWarning],
    ) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            kind,
            root: root.to_string_lossy().into_owned(),
            documents,
            warnings: warnings.iter().map(ScanWarningDto::from).collect(),
        }
    }
}

#[derive(Debug, Serialize)]
struct ScanWarningDto {
    path: String,
    message: String,
}

impl From<&ScanWarning> for ScanWarningDto {
    fn from(warning: &ScanWarning) -> Self {
        Self {
            path: warning.path.to_string_lossy().into_owned(),
            message: warning.message.clone(),
        }
    }
}

#[derive(Debug, Serialize)]
pub(crate) struct PlanDto {
    schema_version: u8,
    kind: &'static str,
    output_root: String,
    operations: Vec<PlanOperationDto>,
}

impl PlanDto {
    pub(crate) fn new(kind: &'static str, output_root: &Path, plan: &OutputPlan) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            kind,
            output_root: output_root.to_string_lossy().into_owned(),
            operations: plan
                .operations()
                .iter()
                .map(PlanOperationDto::from)
                .collect(),
        }
    }
}

#[derive(Debug, Serialize)]
struct PlanOperationDto {
    operation: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    source: Option<String>,
    target: String,
}

impl From<&OutputOperation> for PlanOperationDto {
    fn from(operation: &OutputOperation) -> Self {
        let operation_name = match operation {
            OutputOperation::CreateDirectory { .. } => "create_directory",
            OutputOperation::WriteBytes { .. } => "write_bytes",
            OutputOperation::Copy { .. } => "copy",
            OutputOperation::Symlink { .. } => "symlink",
            OutputOperation::Hardlink { .. } => "hardlink",
            OutputOperation::Reflink { .. } => "reflink",
        };
        Self {
            operation: operation_name,
            source: operation
                .source()
                .map(|path| path.to_string_lossy().into_owned()),
            target: operation
                .target()
                .expect("output operations always have targets")
                .to_string_lossy()
                .into_owned(),
        }
    }
}
