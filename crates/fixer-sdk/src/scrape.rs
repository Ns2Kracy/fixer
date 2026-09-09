//! High-level scrape workflow over automatic matching and exact provider targets.

use crate::{Fixer, Resolved, SdkError};
use fixer_core::{
    AnimeDocument, AnimeMerger, AnimeSeries, BookWork, Candidate, MediaKind, MergePolicy,
    MetadataDocument, Movie, MovieDocument, MovieMerger, MusicReleaseGroup, ProviderId,
    ProviderTarget, ResolutionWarning, ScrapeSelection, Series, SeriesDocument, SeriesMerger,
    SourceRef,
};
use std::time::SystemTime;

const LOCAL_PROVIDER: &str = "local";

/// One resolved metadata value from a high-level scrape.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum ScrapedMedia {
    Movie(Resolved<Movie>),
    Television(Resolved<Series>),
    Anime(Resolved<AnimeSeries>),
    Music(Resolved<MusicReleaseGroup>),
    Book(Resolved<BookWork>),
}

impl ScrapedMedia {
    /// Returns the resolved media domain.
    pub const fn media_kind(&self) -> MediaKind {
        match self {
            Self::Movie(_) => MediaKind::Movie,
            Self::Television(_) => MediaKind::Television,
            Self::Anime(_) => MediaKind::Anime,
            Self::Music(_) => MediaKind::Music,
            Self::Book(_) => MediaKind::Book,
        }
    }

    /// Returns non-fatal provider and resolution warnings.
    pub fn warnings(&self) -> &[ResolutionWarning] {
        match self {
            Self::Movie(value) => &value.warnings,
            Self::Television(value) => &value.warnings,
            Self::Anime(value) => &value.warnings,
            Self::Music(value) => &value.warnings,
            Self::Book(value) => &value.warnings,
        }
    }
}

/// Completed high-level scrape and the provider identity it selected.
#[derive(Debug, Clone, PartialEq)]
pub struct ScrapeResult {
    selected_target: ProviderTarget,
    media: ScrapedMedia,
}

impl ScrapeResult {
    /// Returns the selected provider identity.
    pub const fn selected_target(&self) -> &ProviderTarget {
        &self.selected_target
    }

    /// Returns the resolved metadata used by output planning.
    pub const fn media(&self) -> &ScrapedMedia {
        &self.media
    }

    /// Consumes the result and returns its resolved metadata.
    pub fn into_media(self) -> ScrapedMedia {
        self.media
    }

    /// Returns non-fatal provider and resolution warnings.
    pub fn warnings(&self) -> &[ResolutionWarning] {
        self.media.warnings()
    }
}

/// Configures one scrape from already scanned local metadata.
#[derive(Clone)]
pub struct Scrape {
    fixer: Fixer,
    scanned: MetadataDocument,
    selection: ScrapeSelection,
}

impl Scrape {
    pub(crate) const fn new(fixer: Fixer, scanned: MetadataDocument) -> Self {
        Self {
            fixer,
            scanned,
            selection: ScrapeSelection::Automatic,
        }
    }

    /// Replaces automatic matching with an explicit provider identity.
    #[must_use]
    pub fn selection(mut self, selection: ScrapeSelection) -> Self {
        self.selection = selection;
        self
    }

    /// Resolves metadata without exposing search candidates to the caller.
    pub async fn resolve(self) -> Result<ScrapeResult, SdkError> {
        match self.selection.clone() {
            ScrapeSelection::Automatic => self.resolve_automatic().await,
            ScrapeSelection::Exact(target) => self.resolve_exact(target).await,
        }
    }

    async fn resolve_automatic(self) -> Result<ScrapeResult, SdkError> {
        let title = primary_title(&self.scanned)?.to_owned();
        match self.scanned {
            MetadataDocument::Movie(movie) => {
                let mut query = self.fixer.movie(title);
                if let Some(year) = movie.release_year() {
                    query = query.year(year);
                }
                let search = query.search().await?;
                let selected_target = target_from_first(search.candidates())?;
                let fetched = ScrapedMedia::Movie(search.select(0)?.fetch_selected().await?);
                let media = merge_automatic(
                    MetadataDocument::Movie(movie),
                    fetched,
                    &selected_target,
                    &self.fixer,
                )?;
                Ok(ScrapeResult {
                    selected_target,
                    media,
                })
            }
            MetadataDocument::Television(series) => {
                let search = self
                    .fixer
                    .television(title)
                    .ordering(series.ordering)
                    .search()
                    .await?;
                let selected_target = target_from_first(search.candidates())?;
                let fetched = ScrapedMedia::Television(search.select(0)?.fetch_selected().await?);
                let media = merge_automatic(
                    MetadataDocument::Television(series),
                    fetched,
                    &selected_target,
                    &self.fixer,
                )?;
                Ok(ScrapeResult {
                    selected_target,
                    media,
                })
            }
            MetadataDocument::Anime(anime) => {
                let search = self.fixer.anime(title).search().await?;
                let selected_target = target_from_first(search.candidates())?;
                let fetched = ScrapedMedia::Anime(search.select(0)?.fetch_selected().await?);
                let media = merge_automatic(
                    MetadataDocument::Anime(anime),
                    fetched,
                    &selected_target,
                    &self.fixer,
                )?;
                Ok(ScrapeResult {
                    selected_target,
                    media,
                })
            }
            MetadataDocument::Music(_) => {
                let search = self.fixer.music(title).search().await?;
                let selected_target = target_from_first(search.candidates())?;
                let media = ScrapedMedia::Music(search.select(0)?.fetch_selected().await?);
                Ok(ScrapeResult {
                    selected_target,
                    media,
                })
            }
            MetadataDocument::Book(_) => {
                let search = self.fixer.book(title).search().await?;
                let selected_target = target_from_first(search.candidates())?;
                let media = ScrapedMedia::Book(search.select(0)?.fetch_selected().await?);
                Ok(ScrapeResult {
                    selected_target,
                    media,
                })
            }
        }
    }

    async fn resolve_exact(self, target: ProviderTarget) -> Result<ScrapeResult, SdkError> {
        let scanned_kind = self.scanned.media_kind();
        if target.media_kind() != scanned_kind {
            return Err(SdkError::ScrapeTargetMediaMismatch {
                scanned: scanned_kind,
                selected: target.media_kind(),
            });
        }
        let remote = self.fixer.fetch_exact(&target).await?;
        let media = merge_exact(self.scanned, remote, &target, &self.fixer)?;
        Ok(ScrapeResult {
            selected_target: target,
            media,
        })
    }
}

fn target_from_first(candidates: &[Candidate]) -> Result<ProviderTarget, SdkError> {
    let candidate = candidates.first().ok_or(SdkError::NoCandidates)?;
    ProviderTarget::new(
        candidate.media_kind(),
        candidate.provider().clone(),
        candidate.external_id().clone(),
    )
    .map_err(SdkError::from)
}

fn primary_title(document: &MetadataDocument) -> Result<&str, SdkError> {
    let titles = match document {
        MetadataDocument::Movie(value) => &value.titles,
        MetadataDocument::Television(value) => &value.titles,
        MetadataDocument::Anime(value) => &value.titles,
        MetadataDocument::Music(value) => &value.titles,
        MetadataDocument::Book(value) => &value.titles,
    };
    titles
        .entries()
        .first()
        .map(|entry| entry.value().as_str())
        .ok_or(SdkError::MissingScrapeTitle)
}

fn merge_automatic(
    scanned: MetadataDocument,
    fetched: ScrapedMedia,
    target: &ProviderTarget,
    fixer: &Fixer,
) -> Result<ScrapedMedia, SdkError> {
    let (remote, inherited_warnings) = match fetched {
        ScrapedMedia::Movie(value) => (MetadataDocument::Movie(value.value), value.warnings),
        ScrapedMedia::Television(value) => {
            (MetadataDocument::Television(value.value), value.warnings)
        }
        ScrapedMedia::Anime(value) => (MetadataDocument::Anime(value.value), value.warnings),
        other => return Ok(other),
    };
    let mut merged = merge_exact(scanned, remote, target, fixer)?;
    match &mut merged {
        ScrapedMedia::Movie(value) => value.warnings.extend(inherited_warnings),
        ScrapedMedia::Television(value) => value.warnings.extend(inherited_warnings),
        ScrapedMedia::Anime(value) => value.warnings.extend(inherited_warnings),
        ScrapedMedia::Music(_) | ScrapedMedia::Book(_) => unreachable!("merge kind is preserved"),
    }
    Ok(merged)
}

fn merge_exact(
    scanned: MetadataDocument,
    remote: MetadataDocument,
    target: &ProviderTarget,
    fixer: &Fixer,
) -> Result<ScrapedMedia, SdkError> {
    let local_source = SourceRef::new(
        ProviderId::new(LOCAL_PROVIDER)?,
        None,
        None,
        SystemTime::now(),
    );
    let remote_source = SourceRef::new(
        target.provider().clone(),
        Some(target.external_id().clone()),
        None,
        SystemTime::now(),
    );
    let policy = merge_policy(fixer, target.provider());
    match (scanned, remote) {
        (MetadataDocument::Movie(local), MetadataDocument::Movie(remote)) => {
            let resolved = MovieMerger::new(policy)
                .merge([
                    MovieDocument::new(local, local_source),
                    MovieDocument::new(remote, remote_source),
                ])
                .map_err(|error| SdkError::Merge(error.to_string()))?;
            Ok(ScrapedMedia::Movie(resolved))
        }
        (MetadataDocument::Television(local), MetadataDocument::Television(remote)) => {
            let resolved = SeriesMerger::new(policy)
                .merge([
                    SeriesDocument::new(local, local_source),
                    SeriesDocument::new(remote, remote_source),
                ])
                .map_err(|error| SdkError::Merge(error.to_string()))?;
            Ok(ScrapedMedia::Television(resolved))
        }
        (MetadataDocument::Anime(local), MetadataDocument::Anime(remote)) => {
            let resolved = AnimeMerger::new(policy)
                .merge([
                    AnimeDocument::new(local, local_source),
                    AnimeDocument::new(remote, remote_source),
                ])
                .map_err(|error| SdkError::Merge(error.to_string()))?;
            Ok(ScrapedMedia::Anime(resolved))
        }
        (local, remote) if local.media_kind() == remote.media_kind() => {
            Err(SdkError::ExactScrapeUnsupported(local.media_kind()))
        }
        _ => Err(SdkError::UnexpectedDocument),
    }
}

fn merge_policy(fixer: &Fixer, selected: &ProviderId) -> MergePolicy {
    let local = ProviderId::new(LOCAL_PROVIDER).expect("static provider ID is valid");
    let mut providers = vec![local];
    providers.extend(
        fixer
            .providers
            .iter()
            .map(|provider| provider.descriptor().id().clone())
            .filter(|provider| provider.as_str() != LOCAL_PROVIDER),
    );
    if !providers.contains(selected) {
        providers.push(selected.clone());
    }
    MergePolicy::new(providers)
}
