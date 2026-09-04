use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
};

use fixer_core::{
    LocalizedValue, MetadataDocument, Movie, MovieRelease, ReleaseDate, ReleaseId, WorkId,
};
use fixer_provider_local::{
    LocalError, identify_path, scan as scan_movies, scan_anime, scan_books, scan_music,
    scan_television,
};
use thiserror::Error;

use super::model::MediaKindMode;
use crate::jobs::model::JobMediaKind;

const MAX_DISCOVERED_ITEMS: usize = 10_000;

/// The bounded result of inspecting one logical source entry.
#[derive(Debug, Clone, PartialEq)]
pub enum DiscoveryOutcome {
    Ready(MetadataDocument),
    NeedsReview { media_kinds: Vec<JobMediaKind> },
    Ignored,
}

/// One canonical logical source root and its discovery outcome.
#[derive(Debug, Clone, PartialEq)]
pub struct DiscoveredItem {
    source_root: PathBuf,
    outcome: DiscoveryOutcome,
}

impl DiscoveredItem {
    pub fn source_root(&self) -> &Path {
        &self.source_root
    }

    pub const fn outcome(&self) -> &DiscoveryOutcome {
        &self.outcome
    }

    pub const fn document(&self) -> Option<&MetadataDocument> {
        match &self.outcome {
            DiscoveryOutcome::Ready(document) => Some(document),
            DiscoveryOutcome::NeedsReview { .. } | DiscoveryOutcome::Ignored => None,
        }
    }

    pub const fn media_kind(&self) -> Option<JobMediaKind> {
        match &self.outcome {
            DiscoveryOutcome::Ready(document) => Some(document_kind(document)),
            DiscoveryOutcome::NeedsReview { .. } | DiscoveryOutcome::Ignored => None,
        }
    }
}

#[derive(Debug, Error)]
pub enum DiscoveryError {
    #[error("directory discovery failed: {0}")]
    Local(#[from] fixer_provider_local::LocalError),
    #[error("directory discovery I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("scanner returned mismatched document and root counts for {0:?}")]
    MisalignedScannerOutput(JobMediaKind),
    #[error("scanner returned a root outside the configured source directory")]
    EscapedSourceRoot,
    #[error(
        "directory contains more than {limit} discoverable entries; split it into smaller source folders"
    )]
    LimitExceeded { limit: usize },
}

#[derive(Debug)]
struct Claim {
    root: PathBuf,
    document: MetadataDocument,
}

/// Discovers one item per canonical logical media root beneath `source`.
pub fn discover(
    source: impl AsRef<Path>,
    mode: MediaKindMode,
) -> Result<Vec<DiscoveredItem>, DiscoveryError> {
    let source = source.as_ref().canonicalize()?;
    if !source.is_dir() {
        return Err(fixer_provider_local::LocalError::InvalidPath(source).into());
    }

    let mut claims = match mode {
        MediaKindMode::Fixed(kind) => scan_kind(&source, kind)?,
        MediaKindMode::Auto => {
            let mut claims = Vec::new();
            for kind in ordered_media_kinds() {
                claims.extend(scan_kind(&source, kind)?);
                ensure_within_limit(claims.len())?;
            }
            claims
        }
    };
    ensure_within_limit(claims.len())?;
    if matches!(
        mode,
        MediaKindMode::Fixed(JobMediaKind::Movie) | MediaKindMode::Auto
    ) {
        add_filename_movie_claims(&source, &mut claims)?;
    }

    let mut grouped = BTreeMap::<PathBuf, Vec<MetadataDocument>>::new();
    for claim in claims {
        let root = claim.root.canonicalize()?;
        if !root.starts_with(&source) {
            return Err(DiscoveryError::EscapedSourceRoot);
        }
        ensure_within_limit(grouped.len().saturating_add(1))?;
        let documents = grouped.entry(root).or_default();
        let kind = document_kind(&claim.document);
        if !documents
            .iter()
            .any(|document| document_kind(document) == kind)
        {
            documents.push(claim.document);
        }
    }

    let mut items = grouped
        .into_iter()
        .map(|(source_root, mut documents)| {
            documents.sort_by_key(|document| media_kind_rank(document_kind(document)));
            let outcome = if documents.len() == 1 {
                DiscoveryOutcome::Ready(documents.remove(0))
            } else {
                DiscoveryOutcome::NeedsReview {
                    media_kinds: documents.iter().map(document_kind).collect(),
                }
            };
            DiscoveredItem {
                source_root,
                outcome,
            }
        })
        .collect::<Vec<_>>();

    add_ignored_entries(&source, &mut items)?;
    items.sort_by(|left, right| left.source_root.cmp(&right.source_root));
    Ok(items)
}

/// Finds the deepest discovered root containing an event path.
pub fn reconcile_event(
    items: &[DiscoveredItem],
    changed_path: impl AsRef<Path>,
) -> Result<Option<&DiscoveredItem>, DiscoveryError> {
    let changed_path = canonicalize_with_missing_tail(changed_path.as_ref())?;
    Ok(items
        .iter()
        .filter(|item| changed_path.starts_with(&item.source_root))
        .max_by_key(|item| item.source_root.components().count()))
}

fn scan_kind(source: &Path, kind: JobMediaKind) -> Result<Vec<Claim>, DiscoveryError> {
    match kind {
        JobMediaKind::Movie => {
            let result = scan_movies(source)?;
            zip_claims(
                kind,
                result.roots,
                result.documents,
                MetadataDocument::Movie,
            )
        }
        JobMediaKind::Television => {
            let result = scan_television(source)?;
            zip_claims(
                kind,
                result.roots,
                result.documents,
                MetadataDocument::Television,
            )
        }
        JobMediaKind::Anime => {
            let result = scan_anime(source)?;
            zip_claims(
                kind,
                result.roots,
                result.documents,
                MetadataDocument::Anime,
            )
        }
        JobMediaKind::Music => {
            let result = scan_music(source)?;
            zip_claims(
                kind,
                result.roots,
                result.documents,
                MetadataDocument::Music,
            )
        }
        JobMediaKind::Book => {
            let result = scan_books(source)?;
            zip_claims(kind, result.roots, result.documents, MetadataDocument::Book)
        }
    }
}

fn zip_claims<T>(
    kind: JobMediaKind,
    roots: Vec<PathBuf>,
    documents: Vec<T>,
    wrap: impl Fn(T) -> MetadataDocument,
) -> Result<Vec<Claim>, DiscoveryError> {
    if roots.len() != documents.len() {
        return Err(DiscoveryError::MisalignedScannerOutput(kind));
    }
    Ok(roots
        .into_iter()
        .zip(documents)
        .map(|(root, document)| Claim {
            root,
            document: wrap(document),
        })
        .collect())
}

fn add_ignored_entries(
    source: &Path,
    items: &mut Vec<DiscoveredItem>,
) -> Result<(), DiscoveryError> {
    let roots = items
        .iter()
        .map(|item| item.source_root.clone())
        .collect::<Vec<_>>();
    let source_claimed = roots.iter().any(|root| root == source);
    if source_claimed {
        return Ok(());
    }

    let mut entries = Vec::new();
    for entry in fs::read_dir(source)? {
        ensure_within_limit(entries.len().saturating_add(1))?;
        entries.push(entry?);
    }
    entries.sort_by_key(fs::DirEntry::file_name);
    for entry in entries {
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path)?;
        let path = if metadata.file_type().is_symlink() {
            path
        } else {
            path.canonicalize()?
        };
        let claimed = roots
            .iter()
            .any(|root| root == &path || root.starts_with(&path) || path.starts_with(root));
        if !claimed {
            ensure_within_limit(items.len().saturating_add(1))?;
            items.push(DiscoveredItem {
                source_root: path,
                outcome: DiscoveryOutcome::Ignored,
            });
        }
    }
    Ok(())
}

fn add_filename_movie_claims(source: &Path, claims: &mut Vec<Claim>) -> Result<(), DiscoveryError> {
    let covered_roots = claims
        .iter()
        .map(|claim| claim.root.clone())
        .collect::<Vec<_>>();
    let mut files = Vec::new();
    collect_video_files(source, &mut files)?;
    files.sort();
    for path in files {
        if covered_roots.iter().any(|root| path.starts_with(root)) {
            continue;
        }
        ensure_within_limit(claims.len().saturating_add(1))?;
        claims.push(Claim {
            document: MetadataDocument::Movie(movie_from_filename(&path)?),
            root: path,
        });
    }
    Ok(())
}

fn collect_video_files(directory: &Path, files: &mut Vec<PathBuf>) -> Result<(), DiscoveryError> {
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path)?;
        if metadata.file_type().is_symlink() {
            continue;
        }
        if metadata.is_dir() {
            collect_video_files(&path, files)?;
        } else if is_video(&path) {
            ensure_within_limit(files.len().saturating_add(1))?;
            files.push(path.canonicalize()?);
        }
    }
    Ok(())
}

fn movie_from_filename(path: &Path) -> Result<Movie, LocalError> {
    let hint = identify_path(path)?;
    let slug = hint
        .title
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    let slug = if slug.is_empty() { "movie" } else { &slug };
    let mut titles = LocalizedValue::new();
    titles.insert("und", hint.title)?;
    let mut movie = Movie::new(WorkId::new(format!("local-{slug}"))?, titles);
    if let Some(year) = hint.year {
        movie.releases.push(MovieRelease::new(
            ReleaseId::new(format!("local-{slug}-{year}"))?,
            ReleaseDate::year(year)?,
        ));
    }
    Ok(movie)
}

fn is_video(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|value| value.to_str())
            .map(str::to_ascii_lowercase)
            .as_deref(),
        Some("mkv" | "mp4" | "m4v" | "avi" | "mov")
    )
}

const fn ensure_within_limit(count: usize) -> Result<(), DiscoveryError> {
    if count > MAX_DISCOVERED_ITEMS {
        Err(DiscoveryError::LimitExceeded {
            limit: MAX_DISCOVERED_ITEMS,
        })
    } else {
        Ok(())
    }
}

fn canonicalize_with_missing_tail(path: &Path) -> Result<PathBuf, std::io::Error> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };
    let mut existing = absolute.as_path();
    let mut tail = Vec::<OsString>::new();
    loop {
        match existing.canonicalize() {
            Ok(mut canonical) => {
                for component in tail.iter().rev() {
                    canonical.push(component);
                }
                return Ok(canonical);
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let Some(name) = existing.file_name() else {
                    return Err(error);
                };
                tail.push(name.to_os_string());
                let Some(parent) = existing.parent() else {
                    return Err(error);
                };
                existing = parent;
            }
            Err(error) => return Err(error),
        }
    }
}

const fn document_kind(document: &MetadataDocument) -> JobMediaKind {
    match document {
        MetadataDocument::Movie(_) => JobMediaKind::Movie,
        MetadataDocument::Television(_) => JobMediaKind::Television,
        MetadataDocument::Anime(_) => JobMediaKind::Anime,
        MetadataDocument::Music(_) => JobMediaKind::Music,
        MetadataDocument::Book(_) => JobMediaKind::Book,
    }
}

const fn ordered_media_kinds() -> [JobMediaKind; 5] {
    [
        JobMediaKind::Movie,
        JobMediaKind::Television,
        JobMediaKind::Anime,
        JobMediaKind::Music,
        JobMediaKind::Book,
    ]
}

const fn media_kind_rank(kind: JobMediaKind) -> u8 {
    match kind {
        JobMediaKind::Movie => 0,
        JobMediaKind::Television => 1,
        JobMediaKind::Anime => 2,
        JobMediaKind::Music => 3,
        JobMediaKind::Book => 4,
    }
}
