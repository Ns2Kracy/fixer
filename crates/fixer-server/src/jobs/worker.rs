use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use fixer_core::{
    AssetKind, Isbn13, LocalizedValue, Movie, MovieRelease, ReleaseDate, ReleaseId, WorkId,
};
use fixer_provider_local::{
    LocalProvider, identify_path, parse_json, parse_nfo, scan, scan_anime, scan_books, scan_music,
    scan_television,
};
use fixer_sdk::{AnimeSearch, BookSearch, Fixer, MovieSearch, MusicSearch, TelevisionSearch};
use thiserror::Error;
use tokio::{sync::watch, task::JoinHandle};

use crate::jobs::model::{JobInputDto, JobMediaKind};

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
}

pub(crate) enum SearchArtifact {
    Anime {
        search: AnimeSearch,
        count: u64,
    },
    Book {
        search: BookSearch,
        count: u64,
    },
    Movie {
        search: MovieSearch,
        count: u64,
    },
    Music {
        search: MusicSearch,
        count: u64,
    },
    Television {
        search: TelevisionSearch,
        count: u64,
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
        let artifact = match self.media_kind {
            JobMediaKind::Anime => {
                let search = self.fixer.anime(self.title).search().await?;
                SearchArtifact::Anime {
                    count: count(search.candidates().len())?,
                    search,
                }
            }
            JobMediaKind::Book => {
                let mut query = self.fixer.book(self.title);
                if let Some(isbn) = self.isbn {
                    query = query.isbn(isbn);
                }
                let search = query.search().await?;
                SearchArtifact::Book {
                    count: count(search.candidates().len())?,
                    search,
                }
            }
            JobMediaKind::Movie => {
                let search = self.fixer.movie(self.title).search().await?;
                SearchArtifact::Movie {
                    count: count(search.candidates().len())?,
                    search,
                }
            }
            JobMediaKind::Music => {
                let search = self.fixer.music(self.title).search().await?;
                SearchArtifact::Music {
                    count: count(search.candidates().len())?,
                    search,
                }
            }
            JobMediaKind::Television => {
                let search = self.fixer.television(self.title).search().await?;
                SearchArtifact::Television {
                    count: count(search.candidates().len())?,
                    search,
                }
            }
        };
        Ok(artifact)
    }
}

impl SearchArtifact {
    pub(crate) async fn resolve(self) -> Result<SearchSummary, JobFlowError> {
        let (candidate_count, conflict_count) = match self {
            Self::Anime {
                search,
                count: candidate_count,
            } => (
                candidate_count,
                count(search.select(0)?.fetch_selected().await?.conflicts.len())?,
            ),
            Self::Book {
                search,
                count: candidate_count,
            } => (
                candidate_count,
                count(search.select(0)?.fetch_selected().await?.conflicts.len())?,
            ),
            Self::Movie {
                search,
                count: candidate_count,
            } => (
                candidate_count,
                count(search.select(0)?.fetch_selected().await?.conflicts.len())?,
            ),
            Self::Music {
                search,
                count: candidate_count,
            } => (
                candidate_count,
                count(search.select(0)?.fetch_selected().await?.conflicts.len())?,
            ),
            Self::Television {
                search,
                count: candidate_count,
            } => (
                candidate_count,
                count(search.select(0)?.fetch_selected().await?.conflicts.len())?,
            ),
        };
        Ok(SearchSummary::new(candidate_count, conflict_count))
    }
}

fn scan_local(input: &JobInputDto) -> Result<ScannedJob, JobFlowError> {
    let input_path = PathBuf::from(input.input_path());
    let root = scan_root(&input_path)?;
    let (provider, title, isbn) = match input.media_kind() {
        JobMediaKind::Anime => {
            let result = scan_anime(&root).map_err(local_error)?;
            let document = select_rooted(result.documents, result.roots, &input_path)?;
            let title = title_of(&document.titles)?;
            (
                LocalProvider::from_anime_documents([document]).map_err(local_error)?,
                title,
                None,
            )
        }
        JobMediaKind::Book => select_book(&input_path, &root)?,
        JobMediaKind::Movie => select_movie(&input_path, &root)?,
        JobMediaKind::Music => {
            let result = scan_music(&root).map_err(local_error)?;
            let document = select_rooted(result.documents, result.roots, &input_path)?;
            let title = title_of(&document.titles)?;
            (
                LocalProvider::from_music_documents([document]).map_err(local_error)?,
                title,
                None,
            )
        }
        JobMediaKind::Television => {
            let result = scan_television(&root).map_err(local_error)?;
            let document = select_rooted(result.documents, result.roots, &input_path)?;
            let title = title_of(&document.titles)?;
            (
                LocalProvider::from_television_documents([document]).map_err(local_error)?,
                title,
                None,
            )
        }
    };
    let fixer = Fixer::builder().provider(provider).offline().build()?;
    Ok(ScannedJob {
        fixer,
        media_kind: input.media_kind(),
        title,
        isbn,
    })
}

fn select_book(
    input_path: &Path,
    root: &Path,
) -> Result<(LocalProvider, String, Option<Isbn13>), JobFlowError> {
    let result = scan_books(root).map_err(local_error)?;
    let (work, isbn) = if input_path.is_file() {
        let input = input_path.to_string_lossy();
        let mut matches = result.documents.into_iter().filter_map(|work| {
            let isbn = work.editions.iter().find_map(|edition| {
                edition
                    .assets
                    .iter()
                    .any(|asset| {
                        asset.kind == AssetKind::BookFile && asset.source_path.as_str() == input
                    })
                    .then(|| edition.isbn_13.clone())
            })?;
            Some((work, isbn))
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
        let [work] = result.documents.as_slice() else {
            return Err(ambiguous("book works", result.documents.len()));
        };
        let [edition] = work.editions.as_slice() else {
            return Err(ambiguous("book editions", work.editions.len()));
        };
        (work.clone(), edition.isbn_13.clone())
    };
    let title = title_of(&work.titles)?;
    Ok((
        LocalProvider::from_book_documents([work]).map_err(local_error)?,
        title,
        Some(isbn),
    ))
}

fn select_movie(
    input_path: &Path,
    root: &Path,
) -> Result<(LocalProvider, String, Option<Isbn13>), JobFlowError> {
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
) -> Result<T, JobFlowError> {
    if input_path.is_dir() {
        let count = documents.len();
        if count != 1 {
            return Err(ambiguous("media documents", count));
        }
        return documents
            .into_iter()
            .next()
            .ok_or_else(|| ambiguous("media documents", 0));
    }
    let mut matches = documents
        .into_iter()
        .zip(roots)
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
    let (document, _) = deepest_matches.next().expect("deepest match exists");
    if deepest_matches.next().is_some() {
        return Err(JobFlowError::InvalidInput(
            "input path belongs to multiple scanned media roots".to_owned(),
        ));
    }
    Ok(document)
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
    #[error("candidate count exceeds the persistent summary range")]
    CountOverflow,
}

/// Owns fixed worker tasks and supports cooperative, awaited shutdown.
#[must_use = "retain the worker pool while jobs should run"]
pub struct WorkerPool {
    shutdown: watch::Sender<bool>,
    handles: Vec<JoinHandle<()>>,
}

impl WorkerPool {
    pub(crate) fn new(shutdown: watch::Sender<bool>, handles: Vec<JoinHandle<()>>) -> Self {
        Self { shutdown, handles }
    }
    pub fn worker_count(&self) -> usize {
        self.handles.len()
    }
    pub async fn shutdown(mut self) {
        let _ = self.shutdown.send(true);
        for handle in self.handles.drain(..) {
            let _ = handle.await;
        }
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
