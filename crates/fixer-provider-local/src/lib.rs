//! Local path identification and sidecar metadata provider.

#![forbid(unsafe_code)]

mod anime;
mod identify;
mod json;
mod nfo;
mod television;

pub use anime::{AnimeScanResult, scan_anime};
pub use identify::{EvidenceKind, HintEvidence, MediaHint, identify_path};
pub use json::parse_json;
pub use nfo::parse_nfo;
pub use television::{
    EpisodeHint, MatroskaEpisodeTags, TelevisionScanResult, episode_series_root,
    identify_episode_path, parse_matroska_tags, scan_television,
};

use fixer_core::{
    AnimeCandidate, AnimeSeries, BoxFuture, Candidate, CoreError, ExternalId, FetchRequest,
    HttpClient, MediaKind, MetadataDocument, Movie, MovieCandidate, Provider, ProviderDescriptor,
    ProviderError, ProviderId, SearchRequest, Series, TelevisionCandidate,
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
        let extension = path
            .extension()
            .and_then(|value| value.to_str())
            .map(str::to_ascii_lowercase);
        if !matches!(extension.as_deref(), Some("json" | "nfo")) {
            continue;
        }
        match fs::read_to_string(&path)
            .map_err(LocalError::from)
            .and_then(|input| match extension.as_deref() {
                Some("json") => parse_json(&input).map(Some),
                Some("nfo") if is_non_movie_nfo(&input) => Ok(None),
                Some("nfo") => parse_nfo(&input).map(Some),
                _ => unreachable!("extension was filtered above"),
            }) {
            Ok(Some(document)) => result.documents.push(document),
            Ok(None) => {}
            Err(error) => result.warnings.push(ScanWarning {
                path,
                message: error.to_string(),
            }),
        }
    }
    Ok(())
}

fn is_non_movie_nfo(input: &str) -> bool {
    let input = input.trim_start_matches('\u{feff}').trim_start();
    let root = input
        .strip_prefix("<?xml")
        .and_then(|rest| rest.split_once("?>").map(|(_, body)| body))
        .unwrap_or(input)
        .trim_start();
    [
        "<tvshow",
        "<season",
        "<episodedetails",
        "<anime",
        "<courdetails",
    ]
    .iter()
    .any(|tag| root.starts_with(tag))
}

/// Network-free local metadata provider.
#[derive(Debug, Clone)]
pub struct LocalProvider {
    descriptor: ProviderDescriptor,
    movie_documents: Vec<(ExternalId, Movie)>,
    television_documents: Vec<(ExternalId, Series)>,
    anime_documents: Vec<(ExternalId, AnimeSeries)>,
}

impl LocalProvider {
    /// Constructs a provider from already parsed local movie documents.
    pub fn from_documents(documents: impl IntoIterator<Item = Movie>) -> Result<Self, LocalError> {
        let movie_documents = documents
            .into_iter()
            .map(|movie| ExternalId::new("local", movie.id.as_str()).map(|id| (id, movie)))
            .collect::<Result<Vec<_>, _>>()?;
        Self::from_media_documents(movie_documents, Vec::new(), Vec::new())
    }

    /// Constructs a provider from already parsed local television documents.
    pub fn from_television_documents(
        documents: impl IntoIterator<Item = Series>,
    ) -> Result<Self, LocalError> {
        let television_documents = documents
            .into_iter()
            .map(|series| ExternalId::new("local", series.id.as_str()).map(|id| (id, series)))
            .collect::<Result<Vec<_>, _>>()?;
        Self::from_media_documents(Vec::new(), television_documents, Vec::new())
    }

    /// Constructs a provider from already parsed local anime documents.
    pub fn from_anime_documents(
        documents: impl IntoIterator<Item = AnimeSeries>,
    ) -> Result<Self, LocalError> {
        let anime_documents = documents
            .into_iter()
            .map(|anime| ExternalId::new("local", anime.id.as_str()).map(|id| (id, anime)))
            .collect::<Result<Vec<_>, _>>()?;
        Self::from_media_documents(Vec::new(), Vec::new(), anime_documents)
    }

    /// Scans a root and constructs a provider plus scan warnings.
    pub fn from_scan(root: &Path) -> Result<(Self, Vec<ScanWarning>), LocalError> {
        let movie_result = scan(root)?;
        let (television_records, television_warnings) = television::scan_television_records(root)?;
        let anime_result = anime::scan_anime(root)?;
        let mut warnings = movie_result.warnings;
        warnings.extend(television_warnings);
        warnings.extend(anime_result.warnings);
        let movie_documents = movie_result
            .documents
            .into_iter()
            .map(|movie| ExternalId::new("local", movie.id.as_str()).map(|id| (id, movie)))
            .collect::<Result<Vec<_>, _>>()?;
        let television_documents = television_records
            .into_iter()
            .map(|record| (record.external_id, record.value))
            .collect();
        let anime_documents = anime_result
            .documents
            .into_iter()
            .map(|anime| ExternalId::new("local", anime.id.as_str()).map(|id| (id, anime)))
            .collect::<Result<Vec<_>, _>>()?;
        Ok((
            Self::from_media_documents(movie_documents, television_documents, anime_documents)?,
            warnings,
        ))
    }

    fn from_media_documents(
        movie_documents: Vec<(ExternalId, Movie)>,
        television_documents: Vec<(ExternalId, Series)>,
        anime_documents: Vec<(ExternalId, AnimeSeries)>,
    ) -> Result<Self, LocalError> {
        let mut media_kinds = Vec::new();
        if !movie_documents.is_empty()
            || (television_documents.is_empty() && anime_documents.is_empty())
        {
            media_kinds.push(MediaKind::Movie);
        }
        if !television_documents.is_empty() {
            media_kinds.push(MediaKind::Television);
        }
        if !anime_documents.is_empty() {
            media_kinds.push(MediaKind::Anime);
        }
        let descriptor =
            ProviderDescriptor::new(ProviderId::new("local")?, "Local metadata", media_kinds)?
                .with_network_requirement(false);
        Ok(Self {
            descriptor,
            movie_documents,
            television_documents,
            anime_documents,
        })
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
            let media_kind = request.media_kind();
            self.descriptor.ensure_support(media_kind)?;
            match request {
                SearchRequest::Movie { year, .. } => self
                    .movie_documents
                    .iter()
                    .map(|(external_id, movie)| {
                        let title = first_title(&movie.titles, "local movie has no title")?;
                        MovieCandidate::new(
                            self.descriptor.id().clone(),
                            external_id.clone(),
                            title,
                            movie.release_year().or(year),
                        )
                        .map(Candidate::Movie)
                        .map_err(ProviderError::from)
                    })
                    .collect(),
                SearchRequest::Television { year, .. } => self
                    .television_documents
                    .iter()
                    .map(|(external_id, series)| {
                        let title = first_title(&series.titles, "local series has no title")?;
                        TelevisionCandidate::new(
                            self.descriptor.id().clone(),
                            external_id.clone(),
                            title,
                            year,
                        )
                        .map(Candidate::Television)
                        .map_err(ProviderError::from)
                    })
                    .collect(),
                SearchRequest::Anime { title, year, .. } => self
                    .anime_documents
                    .iter()
                    .map(|(external_id, anime)| {
                        let title =
                            matching_title(&anime.titles, &title, "local anime has no title")?;
                        AnimeCandidate::new(
                            self.descriptor.id().clone(),
                            external_id.clone(),
                            title,
                            year,
                        )
                        .map(Candidate::Anime)
                        .map_err(ProviderError::from)
                    })
                    .collect(),
                _ => Err(ProviderError::UnsupportedMedia {
                    provider: self.descriptor.id().clone(),
                    media_kind,
                }),
            }
        })
    }
    fn fetch<'a>(
        &'a self,
        request: FetchRequest,
        _http: &'a dyn HttpClient,
    ) -> BoxFuture<'a, Result<MetadataDocument, ProviderError>> {
        Box::pin(async move {
            self.descriptor.ensure_support(request.media_kind())?;
            match request.media_kind() {
                MediaKind::Movie => self
                    .movie_documents
                    .iter()
                    .find(|(id, _)| id == &request.external_id)
                    .map(|(_, movie)| MetadataDocument::Movie(movie.clone()))
                    .ok_or(ProviderError::NotFound),
                MediaKind::Television => self
                    .television_documents
                    .iter()
                    .find(|(id, _)| id == &request.external_id)
                    .map(|(_, series)| MetadataDocument::Television(series.clone()))
                    .ok_or(ProviderError::NotFound),
                MediaKind::Anime => self
                    .anime_documents
                    .iter()
                    .find(|(id, _)| id == &request.external_id)
                    .map(|(_, anime)| MetadataDocument::Anime(anime.clone()))
                    .ok_or(ProviderError::NotFound),
                media_kind => Err(ProviderError::UnsupportedMedia {
                    provider: self.descriptor.id().clone(),
                    media_kind,
                }),
            }
        })
    }
}

fn matching_title(
    titles: &fixer_core::LocalizedValue<String>,
    query: &str,
    error: &str,
) -> Result<String, ProviderError> {
    let normalized_query = query.split_whitespace().collect::<String>().to_lowercase();
    titles
        .entries()
        .iter()
        .find(|entry| {
            entry
                .value()
                .split_whitespace()
                .collect::<String>()
                .to_lowercase()
                == normalized_query
        })
        .map(|entry| entry.value().clone())
        .map(Ok)
        .unwrap_or_else(|| first_title(titles, error))
}

fn first_title(
    titles: &fixer_core::LocalizedValue<String>,
    error: &str,
) -> Result<String, ProviderError> {
    titles
        .entries()
        .first()
        .map(|entry| entry.value().clone())
        .ok_or_else(|| ProviderError::InvalidResponse(error.to_owned()))
}
