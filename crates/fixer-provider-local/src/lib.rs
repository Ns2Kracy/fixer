//! Local path identification and sidecar metadata provider.

#![forbid(unsafe_code)]

mod identify;
mod json;
mod nfo;

pub use identify::{EvidenceKind, HintEvidence, MediaHint, identify_path};
pub use json::parse_json;
pub use nfo::parse_nfo;

use fixer_core::{
    BoxFuture, Candidate, CoreError, ExternalId, FetchRequest, HttpClient, MediaKind,
    MetadataDocument, Movie, MovieCandidate, Provider, ProviderDescriptor, ProviderError,
    ProviderId, SearchRequest,
};
use std::{
    fs,
    path::{Path, PathBuf},
};
use thiserror::Error;

/// Local metadata and scan failures.
#[derive(Debug, Error)]
pub enum LocalError {
    #[error("invalid local path `{0}`")]
    InvalidPath(PathBuf),
    #[error("could not identify media from `{0}`")]
    Unidentified(PathBuf),
    #[error("invalid local metadata: {0}")]
    InvalidMetadata(String),
    #[error("local I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("local JSON was invalid: {0}")]
    Json(#[from] serde_json::Error),
    #[error("local NFO XML was invalid: {0}")]
    Xml(#[from] quick_xml::DeError),
    #[error(transparent)]
    Core(#[from] CoreError),
}

/// A non-fatal local scan warning associated with a path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanWarning {
    pub path: PathBuf,
    pub message: String,
}

/// Documents and warnings produced by a local scan.
#[derive(Debug, Clone, Default)]
pub struct ScanResult {
    pub documents: Vec<Movie>,
    pub warnings: Vec<ScanWarning>,
}

type SidecarParser = fn(&str) -> Result<Movie, LocalError>;

/// Recursively scans sidecars without following symbolic links.
pub fn scan(root: &Path) -> Result<ScanResult, LocalError> {
    if !root.is_dir() {
        return Err(LocalError::InvalidPath(root.to_path_buf()));
    }
    let mut result = ScanResult::default();
    scan_directory(root, &mut result)?;
    Ok(result)
}

fn scan_directory(directory: &Path, result: &mut ScanResult) -> Result<(), LocalError> {
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path)?;
        if metadata.file_type().is_symlink() {
            result.warnings.push(ScanWarning {
                path,
                message: "symbolic link skipped during local scan".to_owned(),
            });
            continue;
        }
        if metadata.is_dir() {
            scan_directory(&path, result)?;
            continue;
        }
        let parser: Option<SidecarParser> = match path
            .extension()
            .and_then(|value| value.to_str())
            .map(str::to_ascii_lowercase)
            .as_deref()
        {
            Some("json") => Some(parse_json),
            Some("nfo") => Some(parse_nfo),
            _ => None,
        };
        let Some(parser) = parser else {
            continue;
        };
        match fs::read_to_string(&path)
            .map_err(LocalError::from)
            .and_then(|input| parser(&input))
        {
            Ok(document) => result.documents.push(document),
            Err(error) => result.warnings.push(ScanWarning {
                path,
                message: error.to_string(),
            }),
        }
    }
    Ok(())
}

/// Network-free local metadata provider.
#[derive(Debug, Clone)]
pub struct LocalProvider {
    descriptor: ProviderDescriptor,
    documents: Vec<(ExternalId, Movie)>,
}

impl LocalProvider {
    /// Constructs a provider from already parsed local movie documents.
    pub fn from_documents(documents: impl IntoIterator<Item = Movie>) -> Result<Self, LocalError> {
        let documents = documents
            .into_iter()
            .map(|movie| ExternalId::new("local", movie.id.as_str()).map(|id| (id, movie)))
            .collect::<Result<Vec<_>, _>>()?;
        let descriptor = ProviderDescriptor::new(
            ProviderId::new("local")?,
            "Local metadata",
            [MediaKind::Movie],
        )?
        .with_network_requirement(false);
        Ok(Self {
            descriptor,
            documents,
        })
    }

    /// Scans a root and constructs a provider plus scan warnings.
    pub fn from_scan(root: &Path) -> Result<(Self, Vec<ScanWarning>), LocalError> {
        let result = scan(root)?;
        Ok((Self::from_documents(result.documents)?, result.warnings))
    }
}

impl Provider for LocalProvider {
    fn descriptor(&self) -> &ProviderDescriptor {
        &self.descriptor
    }
    fn search<'a>(
        &'a self,
        request: SearchRequest,
        _http: &'a dyn HttpClient,
    ) -> BoxFuture<'a, Result<Vec<Candidate>, ProviderError>> {
        Box::pin(async move {
            self.descriptor.ensure_support(request.media_kind())?;
            let SearchRequest::Movie { year, .. } = request else {
                return Err(ProviderError::UnsupportedMedia {
                    provider: self.descriptor.id().clone(),
                    media_kind: request.media_kind(),
                });
            };
            self.documents
                .iter()
                .map(|(external_id, movie)| {
                    let title = movie
                        .titles
                        .entries()
                        .first()
                        .map(|entry| entry.value().clone())
                        .ok_or_else(|| {
                            ProviderError::InvalidResponse("local movie has no title".to_owned())
                        })?;
                    MovieCandidate::new(
                        self.descriptor.id().clone(),
                        external_id.clone(),
                        title,
                        movie.release_year().or(year),
                    )
                    .map(Candidate::Movie)
                    .map_err(ProviderError::from)
                })
                .collect()
        })
    }
    fn fetch<'a>(
        &'a self,
        request: FetchRequest,
        _http: &'a dyn HttpClient,
    ) -> BoxFuture<'a, Result<MetadataDocument, ProviderError>> {
        Box::pin(async move {
            self.descriptor.ensure_support(request.media_kind())?;
            self.documents
                .iter()
                .find(|(id, _)| id == &request.external_id)
                .map(|(_, movie)| MetadataDocument::Movie(movie.clone()))
                .ok_or(ProviderError::NotFound)
        })
    }
}
