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
