//! Ergonomic typed anime query flow.

use crate::{Fixer, SdkError, orchestrator};
use fixer_core::{AnimeSeries, Candidate, ExternalId, ResolutionWarning, Resolved};

/// A typed anime series query builder.
#[derive(Clone)]
pub struct AnimeQuery {
    fixer: Fixer,
    title: String,
    year: Option<u16>,
    external_ids: Vec<ExternalId>,
}

impl AnimeQuery {
    pub(crate) const fn new(fixer: Fixer, title: String) -> Self {
        Self {
            fixer,
            title,
            year: None,
            external_ids: Vec::new(),
        }
    }

    /// Restricts matching to a first-air year.
    pub const fn year(mut self, year: u16) -> Self {
        self.year = Some(year);
        self
    }

    /// Adds an exact provider external ID to matching evidence.
    pub fn external_id(mut self, external_id: ExternalId) -> Self {
        if !self.external_ids.contains(&external_id) {
            self.external_ids.push(external_id);
        }
        self
    }

    /// Searches providers and returns deterministic ranked anime candidates.
    pub async fn search(self) -> Result<AnimeSearch, SdkError> {
        let outcome =
            orchestrator::search_anime(&self.fixer, &self.title, self.year, &self.external_ids)
                .await?;
        Ok(AnimeSearch {
            fixer: self.fixer,
            candidates: outcome.candidates,
            warnings: outcome.warnings,
            external_ids: self.external_ids,
        })
    }

    /// Searches and fetches the deterministic top anime candidate.
    pub async fn resolve(self) -> Result<Resolved<AnimeSeries>, SdkError> {
        let search = self.search().await?;
        orchestrator::fetch_anime(
            &search.fixer,
            &search.candidates,
            search.warnings,
            &search.external_ids,
        )
        .await
    }
}

/// Ranked anime search results.
pub struct AnimeSearch {
    fixer: Fixer,
    candidates: Vec<Candidate>,
    warnings: Vec<ResolutionWarning>,
    external_ids: Vec<ExternalId>,
}

impl AnimeSearch {
    /// Returns ranked candidates.
    pub fn candidates(&self) -> &[Candidate] {
        &self.candidates
    }

    /// Returns non-fatal provider and matching warnings from the search.
    pub fn warnings(&self) -> &[ResolutionWarning] {
        &self.warnings
    }

    /// Selects one candidate explicitly.
    pub fn select(self, index: usize) -> Result<SelectedAnime, SdkError> {
        let length = self.candidates.len();
        let candidate = self
            .candidates
            .into_iter()
            .nth(index)
            .ok_or(SdkError::CandidateOutOfBounds { index, length })?;
        Ok(SelectedAnime {
            fixer: self.fixer,
            candidate,
            warnings: self.warnings,
            external_ids: self.external_ids,
        })
    }
}

/// One explicit anime candidate ready to fetch.
pub struct SelectedAnime {
    fixer: Fixer,
    candidate: Candidate,
    warnings: Vec<ResolutionWarning>,
    external_ids: Vec<ExternalId>,
}

impl SelectedAnime {
    /// Fetches the explicitly selected anime candidate.
    pub async fn fetch_selected(self) -> Result<Resolved<AnimeSeries>, SdkError> {
        orchestrator::fetch_anime(
            &self.fixer,
            &[self.candidate],
            self.warnings,
            &self.external_ids,
        )
        .await
    }
}
