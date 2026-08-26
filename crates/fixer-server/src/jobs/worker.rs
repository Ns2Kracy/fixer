use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use fixer_core::{
    AnimeSeries, AssetKind, BookWork, Isbn13, LocalizedValue, Movie, MovieRelease,
    MusicReleaseGroup, OutputPlan, ReleaseDate, ReleaseId, Resolved, Series, WorkId,
};
use fixer_provider_local::{
    LocalProvider, identify_path, parse_json, parse_nfo, scan, scan_anime, scan_books, scan_music,
    scan_television,
};
use fixer_sdk::{AnimeSearch, BookSearch, Fixer, MovieSearch, MusicSearch, TelevisionSearch};
use fixer_writer_local::{AnimeWriter, BookWriter, JsonWriter, MusicWriter, TelevisionWriter};
use sha2::{Digest, Sha256};
use thiserror::Error;
use tokio::{sync::watch, task::JoinHandle};

use crate::jobs::{
    ExecutionTaskRegistry,
    model::{JobInputDto, JobMediaKind, ReviewDecisionDto},
};

/// Bounded result retained after SDK resolution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SearchSummary {
    candidate_count: u64,
    conflict_count: u64,
}

impl SearchSummary {
    pub const fn new(candidate_count: u64, conflict_count: u64) -> Self {
        Self {
            candidate_count,
            conflict_count,
        }
    }
    pub const fn candidate_count(self) -> u64 {
        self.candidate_count
    }
    pub const fn conflict_count(self) -> u64 {
        self.conflict_count
    }
}

/// Reusable configured SDK flow, including deterministic Fixture providers.
#[derive(Clone)]
pub struct SdkJobFlow {
    fixer: Fixer,
}
impl SdkJobFlow {
    pub const fn new(fixer: Fixer) -> Self {
        Self { fixer }
    }
}

#[derive(Clone)]
pub(crate) enum WorkerFlow {
    Configured(SdkJobFlow),
    Local,
}

pub(crate) struct ScannedJob {
    fixer: Fixer,
    media_kind: JobMediaKind,
    title: String,
    isbn: Option<Isbn13>,
    output_root: PathBuf,
}

pub(crate) enum SearchArtifact {
    Anime {
        search: AnimeSearch,
        count: u64,
        output_root: PathBuf,
    },
    Book {
        search: BookSearch,
        count: u64,
        isbn: Option<Isbn13>,
        output_root: PathBuf,
    },
    Movie {
        search: MovieSearch,
        count: u64,
        output_root: PathBuf,
    },
    Music {
        search: MusicSearch,
        count: u64,
        output_root: PathBuf,
    },
    Television {
        search: TelevisionSearch,
        count: u64,
        output_root: PathBuf,
    },
}

pub(crate) enum ResolvedArtifact {
    Anime {
        resolved: Resolved<AnimeSeries>,
        output_root: PathBuf,
    },
    Book {
        resolved: Resolved<BookWork>,
        isbn: Option<Isbn13>,
        output_root: PathBuf,
    },
    Movie {
        resolved: Resolved<Movie>,
        output_root: PathBuf,
    },
    Music {
        resolved: Resolved<MusicReleaseGroup>,
        output_root: PathBuf,
    },
    Television {
        resolved: Resolved<Series>,
        output_root: PathBuf,
    },
}

impl WorkerFlow {
    pub(crate) async fn scan(&self, input: &JobInputDto) -> Result<ScannedJob, JobFlowError> {
        match self {
            Self::Configured(flow) => Ok(ScannedJob {
                fixer: flow.fixer.clone(),
                media_kind: input.media_kind(),
                title: path_query_title(Path::new(input.input_path()))?,
                isbn: None,
                output_root: scan_root(Path::new(input.input_path()))?,
            }),
            Self::Local => {
                let input = input.clone();
                tokio::task::spawn_blocking(move || scan_local(&input))
                    .await
                    .map_err(|error| JobFlowError::BlockingTask(error.to_string()))?
            }
        }
    }
}

impl ScannedJob {
    pub(crate) async fn search(self) -> Result<SearchArtifact, JobFlowError> {
        let Self {
            fixer,
            media_kind,
            title,
            isbn,
            output_root,
        } = self;
        let artifact = match media_kind {
            JobMediaKind::Anime => {
                let search = fixer.anime(title).search().await?;
                SearchArtifact::Anime {
                    count: count(search.candidates().len())?,
                    search,
                    output_root,
                }
            }
            JobMediaKind::Book => {
                let mut query = fixer.book(title);
                if let Some(isbn) = isbn.clone() {
                    query = query.isbn(isbn);
                }
                let search = query.search().await?;
                SearchArtifact::Book {
                    count: count(search.candidates().len())?,
                    search,
                    isbn,
                    output_root,
                }
            }
            JobMediaKind::Movie => {
                let search = fixer.movie(title).search().await?;
                SearchArtifact::Movie {
                    count: count(search.candidates().len())?,
                    search,
                    output_root,
                }
            }
            JobMediaKind::Music => {
                let search = fixer.music(title).search().await?;
                SearchArtifact::Music {
                    count: count(search.candidates().len())?,
                    search,
                    output_root,
                }
            }
            JobMediaKind::Television => {
                let search = fixer.television(title).search().await?;
                SearchArtifact::Television {
                    count: count(search.candidates().len())?,
                    search,
                    output_root,
                }
            }
        };
        Ok(artifact)
    }
}

impl SearchArtifact {
    pub(crate) async fn resolve(self) -> Result<SearchSummary, JobFlowError> {
        let candidate_count = self.candidate_count();
        let resolved = self.resolve_selected(0).await?;
        Ok(SearchSummary::new(
            candidate_count,
            resolved.conflict_count()?,
        ))
    }

    pub(crate) const fn candidate_count(&self) -> u64 {
        match self {
            Self::Anime { count, .. }
            | Self::Book { count, .. }
            | Self::Movie { count, .. }
            | Self::Music { count, .. }
            | Self::Television { count, .. } => *count,
        }
    }

    pub(crate) async fn resolve_selected(
        self,
        candidate_index: u64,
    ) -> Result<ResolvedArtifact, JobFlowError> {
        let index = usize::try_from(candidate_index).map_err(|_| JobFlowError::IndexOverflow)?;
        Ok(match self {
            Self::Anime {
                search,
                output_root,
                ..
            } => ResolvedArtifact::Anime {
                resolved: search.select(index)?.fetch_selected().await?,
                output_root,
            },
            Self::Book {
                search,
                isbn,
                output_root,
                ..
            } => ResolvedArtifact::Book {
                resolved: search.select(index)?.fetch_selected().await?,
                isbn,
                output_root,
            },
            Self::Movie {
                search,
                output_root,
                ..
            } => ResolvedArtifact::Movie {
                resolved: search.select(index)?.fetch_selected().await?,
                output_root,
            },
            Self::Music {
                search,
                output_root,
                ..
            } => ResolvedArtifact::Music {
                resolved: search.select(index)?.fetch_selected().await?,
                output_root,
            },
            Self::Television {
                search,
                output_root,
                ..
            } => ResolvedArtifact::Television {
                resolved: search.select(index)?.fetch_selected().await?,
                output_root,
            },
        })
    }
}

impl ResolvedArtifact {
    pub(crate) fn conflict_count(&self) -> Result<u64, JobFlowError> {
        count(match self {
            Self::Anime { resolved, .. } => resolved.conflicts.len(),
            Self::Book { resolved, .. } => resolved.conflicts.len(),
            Self::Movie { resolved, .. } => resolved.conflicts.len(),
            Self::Music { resolved, .. } => resolved.conflicts.len(),
            Self::Television { resolved, .. } => resolved.conflicts.len(),
        })
    }

    pub(crate) fn plan(&self) -> Result<OutputPlan, JobFlowError> {
        let plan = match self {
            Self::Anime {
                resolved,
                output_root,
            } => AnimeWriter
                .plan_resolved(resolved, output_root)
                .map_err(planning_error)?,
            Self::Book {
                resolved,
                isbn,
                output_root,
            } => {
                let isbn = isbn.clone().or_else(|| {
                    let [edition] = resolved.value.editions.as_slice() else {
                        return None;
                    };
                    Some(edition.isbn_13.clone())
                });
                BookWriter::for_isbn(isbn.ok_or(JobFlowError::MissingBookIsbn)?)
                    .plan_resolved(resolved, output_root)
                    .map_err(planning_error)?
            }
            Self::Movie {
                resolved,
                output_root,
            } => JsonWriter
                .plan_resolved(resolved, output_root)
                .map_err(planning_error)?,
            Self::Music {
                resolved,
                output_root,
            } => MusicWriter::default()
                .plan_resolved(resolved, output_root)
                .map_err(planning_error)?,
            Self::Television {
                resolved,
                output_root,
            } => TelevisionWriter
                .plan_resolved(resolved, output_root)
                .map_err(planning_error)?,
        };
        Ok(plan)
    }

    pub(crate) fn plan_fingerprint(
        &self,
        decision: &ReviewDecisionDto,
        plan: &OutputPlan,
    ) -> Result<String, JobFlowError> {
        let (media_kind, resolved) = match self {
            Self::Anime { resolved, .. } => ("anime", serde_json::to_value(resolved)),
            Self::Book { resolved, .. } => ("book", serde_json::to_value(resolved)),
            Self::Movie { resolved, .. } => ("movie", serde_json::to_value(resolved)),
            Self::Music { resolved, .. } => ("music", serde_json::to_value(resolved)),
            Self::Television { resolved, .. } => ("television", serde_json::to_value(resolved)),
        };
        let mut resolved = resolved.map_err(fingerprint_error)?;
        scrub_observation_times(&mut resolved);
        let mut hasher = Sha256::new();
        hash_frame(&mut hasher, media_kind.as_bytes());
        hash_frame(
            &mut hasher,
            &serde_json::to_vec(decision).map_err(fingerprint_error)?,
        );
        hash_frame(
            &mut hasher,
            &serde_json::to_vec(&resolved).map_err(fingerprint_error)?,
        );
        hash_frame(&mut hasher, plan.output_root.to_string_lossy().as_bytes());
        for operation in plan.operations() {
            match operation {
                fixer_core::OutputOperation::CreateDirectory { target } => {
                    hash_frame(&mut hasher, b"create_directory");
                    hash_path(&mut hasher, target);
                }
                fixer_core::OutputOperation::WriteBytes { target, content } => {
                    hash_frame(&mut hasher, b"write_bytes");
                    hash_path(&mut hasher, target);
                    match serde_json::from_slice::<serde_json::Value>(content.as_bytes()) {
                        Ok(mut json) => {
                            scrub_observation_times(&mut json);
                            hash_frame(
                                &mut hasher,
                                &serde_json::to_vec(&json).map_err(fingerprint_error)?,
                            );
                        }
                        Err(_) => hash_frame(&mut hasher, content.as_bytes()),
                    }
                }
                fixer_core::OutputOperation::Copy { source, target } => {
                    hash_frame(&mut hasher, b"copy");
                    hash_path(&mut hasher, source);
                    hash_path(&mut hasher, target);
                }
                fixer_core::OutputOperation::Symlink { source, target } => {
                    hash_frame(&mut hasher, b"symlink");
                    hash_path(&mut hasher, source);
                    hash_path(&mut hasher, target);
                }
                fixer_core::OutputOperation::Hardlink { source, target } => {
                    hash_frame(&mut hasher, b"hardlink");
                    hash_path(&mut hasher, source);
                    hash_path(&mut hasher, target);
                }
                fixer_core::OutputOperation::Reflink { source, target } => {
                    hash_frame(&mut hasher, b"reflink");
                    hash_path(&mut hasher, source);
                    hash_path(&mut hasher, target);
                }
            }
        }
        let digest = hasher.finalize();
        Ok(digest.iter().map(|byte| format!("{byte:02x}")).collect())
    }
}

fn scrub_observation_times(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            map.remove("observed_at_unix_ms");
            for value in map.values_mut() {
                scrub_observation_times(value);
            }
        }
        serde_json::Value::Array(values) => {
            for value in values {
                scrub_observation_times(value);
            }
        }
        _ => {}
    }
}

fn hash_frame(hasher: &mut Sha256, bytes: &[u8]) {
    hasher.update(u64::try_from(bytes.len()).unwrap_or(u64::MAX).to_be_bytes());
    hasher.update(bytes);
}

fn hash_path(hasher: &mut Sha256, path: &Path) {
    hash_frame(hasher, path.to_string_lossy().as_bytes());
}

fn fingerprint_error(error: impl ToString) -> JobFlowError {
    JobFlowError::Fingerprint(error.to_string())
}

fn scan_local(input: &JobInputDto) -> Result<ScannedJob, JobFlowError> {
    let input_path = PathBuf::from(input.input_path());
    let root = scan_root(&input_path)?;
    let (provider, title, isbn, output_root) = match input.media_kind() {
        JobMediaKind::Anime => {
            let result = scan_anime(&root).map_err(local_error)?;
            let (document, output_root) =
                select_rooted(result.documents, result.roots, &input_path)?;
            let title = title_of(&document.titles)?;
            (
                LocalProvider::from_anime_documents([document]).map_err(local_error)?,
                title,
                None,
                output_root,
            )
        }
        JobMediaKind::Book => select_book(&input_path, &root)?,
        JobMediaKind::Movie => select_movie(&input_path, &root)?,
        JobMediaKind::Music => {
            let result = scan_music(&root).map_err(local_error)?;
            let (document, output_root) =
                select_rooted(result.documents, result.roots, &input_path)?;
            let title = title_of(&document.titles)?;
            (
                LocalProvider::from_music_documents([document]).map_err(local_error)?,
                title,
                None,
                output_root,
            )
        }
        JobMediaKind::Television => {
            let result = scan_television(&root).map_err(local_error)?;
            let (document, output_root) =
                select_rooted(result.documents, result.roots, &input_path)?;
            let title = title_of(&document.titles)?;
            (
                LocalProvider::from_television_documents([document]).map_err(local_error)?,
                title,
                None,
                output_root,
            )
        }
    };
    let fixer = Fixer::builder().provider(provider).offline().build()?;
    Ok(ScannedJob {
        fixer,
        media_kind: input.media_kind(),
        title,
        isbn,
        output_root,
    })
}

fn select_book(
    input_path: &Path,
    root: &Path,
) -> Result<(LocalProvider, String, Option<Isbn13>, PathBuf), JobFlowError> {
    let result = scan_books(root).map_err(local_error)?;
    if result.documents.len() != result.roots.len() {
        return Err(JobFlowError::InvalidInput(
            "book scan returned mismatched documents and roots".to_owned(),
        ));
    }
    let mut pairs = result
        .documents
        .into_iter()
        .zip(result.roots)
        .collect::<Vec<_>>();
    let (work, isbn, output_root) = if input_path.is_file() {
        let input = input_path.to_string_lossy();
        let mut matches = pairs.into_iter().filter_map(|(work, output_root)| {
            let isbn = work.editions.iter().find_map(|edition| {
                edition
                    .assets
                    .iter()
                    .any(|asset| {
                        asset.kind == AssetKind::BookFile && asset.source_path.as_str() == input
                    })
                    .then(|| edition.isbn_13.clone())
            })?;
            Some((work, isbn, output_root))
        });
        let selected = matches.next().ok_or_else(|| {
            JobFlowError::InvalidInput("input EPUB does not match a scanned edition".to_owned())
        })?;
        if matches.next().is_some() {
            return Err(JobFlowError::InvalidInput(
                "input EPUB matches multiple scanned works".to_owned(),
            ));
        }
        selected
    } else {
        if pairs.len() != 1 {
            return Err(ambiguous("book works", pairs.len()));
        }
        let (work, output_root) = pairs.pop().expect("one book work was checked");
        let [edition] = work.editions.as_slice() else {
            return Err(ambiguous("book editions", work.editions.len()));
        };
        let isbn = edition.isbn_13.clone();
        (work, isbn, output_root)
    };
    let title = title_of(&work.titles)?;
    Ok((
        LocalProvider::from_book_documents([work]).map_err(local_error)?,
        title,
        Some(isbn),
        output_root,
    ))
}

fn select_movie(
    input_path: &Path,
    root: &Path,
) -> Result<(LocalProvider, String, Option<Isbn13>, PathBuf), JobFlowError> {
    let movie = if input_path.is_file() {
        let json = input_path.with_extension("json");
        let nfo = input_path.with_extension("nfo");
        if json.is_file() {
            parse_json(&std::fs::read_to_string(json).map_err(local_error)?).map_err(local_error)?
        } else if nfo.is_file() {
            parse_nfo(&std::fs::read_to_string(nfo).map_err(local_error)?).map_err(local_error)?
        } else {
            movie_from_hint(identify_path(input_path).map_err(local_error)?)?
        }
    } else {
        let result = scan(root).map_err(local_error)?;
        let [movie] = result.documents.as_slice() else {
            return Err(ambiguous("movie documents", result.documents.len()));
        };
        movie.clone()
    };
    let title = title_of(&movie.titles)?;
    Ok((
        LocalProvider::from_documents([movie]).map_err(local_error)?,
        title,
        None,
        root.to_owned(),
    ))
}

fn movie_from_hint(hint: fixer_provider_local::MediaHint) -> Result<Movie, JobFlowError> {
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
    titles.insert("und", hint.title).map_err(local_error)?;
    let mut movie = Movie::new(
        WorkId::new(format!("local-{slug}")).map_err(local_error)?,
        titles,
    );
    if let Some(year) = hint.year {
        movie.releases.push(MovieRelease::new(
            ReleaseId::new(format!("local-{slug}-{year}")).map_err(local_error)?,
            ReleaseDate::year(year).map_err(local_error)?,
        ));
    }
    Ok(movie)
}

fn select_rooted<T>(
    documents: Vec<T>,
    roots: Vec<PathBuf>,
    input_path: &Path,
) -> Result<(T, PathBuf), JobFlowError> {
    if documents.len() != roots.len() {
        return Err(JobFlowError::InvalidInput(
            "media scan returned mismatched documents and roots".to_owned(),
        ));
    }
    let pairs = documents.into_iter().zip(roots).collect::<Vec<_>>();
    if input_path.is_dir() {
        let count = pairs.len();
        if count != 1 {
            return Err(ambiguous("media documents", count));
        }
        return pairs
            .into_iter()
            .next()
            .ok_or_else(|| ambiguous("media documents", 0));
    }
    let mut matches = pairs
        .into_iter()
        .filter(|(_, root)| input_path.starts_with(root))
        .collect::<Vec<_>>();
    let deepest = matches
        .iter()
        .map(|(_, root)| root.components().count())
        .max()
        .ok_or_else(|| {
            JobFlowError::InvalidInput(
                "input path does not belong to a scanned media root".to_owned(),
            )
        })?;
    let mut deepest_matches = matches
        .drain(..)
        .filter(|(_, root)| root.components().count() == deepest);
    let (document, root) = deepest_matches.next().expect("deepest match exists");
    if deepest_matches.next().is_some() {
        return Err(JobFlowError::InvalidInput(
            "input path belongs to multiple scanned media roots".to_owned(),
        ));
    }
    Ok((document, root))
}

fn title_of(titles: &fixer_core::Titles) -> Result<String, JobFlowError> {
    titles
        .entries()
        .first()
        .map(|entry| entry.value().clone())
        .ok_or_else(|| JobFlowError::InvalidInput("scanned document has no title".to_owned()))
}

fn ambiguous(kind: &str, count: usize) -> JobFlowError {
    JobFlowError::InvalidInput(format!("expected one {kind}, found {count}"))
}

fn scan_root(path: &Path) -> Result<PathBuf, JobFlowError> {
    if path.is_dir() {
        return Ok(path.to_owned());
    }
    path.parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .map(Path::to_owned)
        .ok_or_else(|| {
            JobFlowError::InvalidInput("input path has no containing directory".to_owned())
        })
}

fn path_query_title(path: &Path) -> Result<String, JobFlowError> {
    let value = if path.is_dir() {
        path.file_name()
    } else {
        path.file_stem().or_else(|| path.file_name())
    };
    value
        .and_then(|value| value.to_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| JobFlowError::InvalidInput("input path has no UTF-8 title".to_owned()))
}

fn count(value: usize) -> Result<u64, JobFlowError> {
    u64::try_from(value).map_err(|_| JobFlowError::CountOverflow)
}
fn local_error(error: impl ToString) -> JobFlowError {
    JobFlowError::Local(error.to_string())
}
fn planning_error(error: impl ToString) -> JobFlowError {
    JobFlowError::Planning(error.to_string())
}

#[derive(Debug, Error)]
pub enum JobFlowError {
    #[error("job input is invalid: {0}")]
    InvalidInput(String),
    #[error("local media scan failed: {0}")]
    Local(String),
    #[error(transparent)]
    Sdk(#[from] fixer_sdk::SdkError),
    #[error("blocking scan task failed: {0}")]
    BlockingTask(String),
    #[error("candidate or conflict index exceeds this platform's addressable range")]
    IndexOverflow,
    #[error("book planning requires an ISBN-13 selected during scan")]
    MissingBookIsbn,
    #[error("output planning failed: {0}")]
    Planning(String),
    #[error("reviewed-plan fingerprint failed: {0}")]
    Fingerprint(String),
    #[error("candidate count exceeds the persistent summary range")]
    CountOverflow,
}

/// Owns fixed worker tasks and supports cooperative, awaited shutdown.
#[must_use = "retain the worker pool while jobs should run"]
pub struct WorkerPool {
    shutdown: watch::Sender<bool>,
    handles: Vec<JoinHandle<()>>,
    execution_tasks: Arc<ExecutionTaskRegistry>,
}

impl WorkerPool {
    pub(crate) fn new(
        shutdown: watch::Sender<bool>,
        handles: Vec<JoinHandle<()>>,
        execution_tasks: Arc<ExecutionTaskRegistry>,
    ) -> Self {
        Self {
            shutdown,
            handles,
            execution_tasks,
        }
    }
    pub fn worker_count(&self) -> usize {
        self.handles.len()
    }
    pub async fn shutdown(mut self) {
        let _ = self.shutdown.send(true);
        for handle in self.handles.drain(..) {
            let _ = handle.await;
        }
        self.execution_tasks.close_and_wait().await;
    }
}

impl Drop for WorkerPool {
    fn drop(&mut self) {
        let _ = self.shutdown.send(true);
    }
}

pub(crate) const RETRY_DELAYS: [Duration; 3] = [
    Duration::from_millis(10),
    Duration::from_millis(25),
    Duration::from_millis(50),
];
pub(crate) type SharedWorkerFlow = Arc<WorkerFlow>;

#[cfg(test)]
mod tests {

    #[tokio::test]
    async fn metadata_free_movie_files_use_filename_hints_through_resolve() {
        let directory = tempfile::tempdir().unwrap();
        let movie = directory.path().join("In.The.Mood.For.Love.2000.mkv");
        std::fs::write(&movie, b"fixture").unwrap();
        let input = crate::jobs::model::JobInputDto::new(
            crate::jobs::model::JobMediaKind::Movie,
            movie.to_string_lossy(),
            false,
        );

        let scanned = super::WorkerFlow::Local.scan(&input).await.unwrap();
        let summary = scanned.search().await.unwrap().resolve().await.unwrap();
        assert_eq!(summary.candidate_count(), 1);
        assert_eq!(summary.conflict_count(), 0);
    }

    #[tokio::test]
    async fn movie_file_uses_its_own_sidecar_despite_sibling_documents() {
        let directory = tempfile::tempdir().unwrap();
        let first = directory.path().join("first.mkv");
        std::fs::write(&first, b"fixture").unwrap();
        std::fs::write(
            first.with_extension("json"),
            include_str!("../../../fixer-provider-local/tests/fixtures/movie.json"),
        )
        .unwrap();
        std::fs::write(
            directory.path().join("sibling.json"),
            include_str!("../../../fixer-provider-local/tests/fixtures/movie.json"),
        )
        .unwrap();
        let input = crate::jobs::model::JobInputDto::new(
            crate::jobs::model::JobMediaKind::Movie,
            first.to_string_lossy(),
            false,
        );

        let scanned = super::WorkerFlow::Local.scan(&input).await.unwrap();
        let summary = scanned.search().await.unwrap().resolve().await.unwrap();
        assert_eq!(summary.candidate_count(), 1);
    }

    #[test]
    fn dotted_directories_keep_their_full_name_as_the_query_title() {
        let directory = tempfile::tempdir().unwrap();
        let media = directory.path().join("In.The.Mood.For.Love");
        std::fs::create_dir(&media).unwrap();
        assert_eq!(
            super::path_query_title(&media).unwrap(),
            "In.The.Mood.For.Love"
        );
    }
}
