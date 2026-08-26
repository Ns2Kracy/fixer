//! Ergonomic typed music release-group query flow.

use crate::{Fixer, SdkError, orchestrator};
use fixer_core::{Candidate, MusicReleaseGroup, ResolutionWarning, Resolved};

/// A typed music release-group query builder.
#[derive(Clone)]
pub struct MusicQuery {
    fixer: Fixer,
    title: String,
    year: Option<u16>,
}

impl MusicQuery {
    pub(crate) const fn new(fixer: Fixer, title: String) -> Self {
        Self {
            fixer,
            title,
            year: None,
        }
    }

    /// Restricts matching to a first-release year.
    pub const fn year(mut self, year: u16) -> Self {
        self.year = Some(year);
        self
    }

    /// Searches providers and returns deterministic ranked music candidates.
    pub async fn search(self) -> Result<MusicSearch, SdkError> {
        let outcome = orchestrator::search_music(&self.fixer, &self.title, self.year).await?;
        Ok(MusicSearch {
            fixer: self.fixer,
            candidates: outcome.candidates,
            warnings: outcome.warnings,
        })
    }

    /// Searches and fetches only the deterministic top music candidate.
    pub async fn resolve(self) -> Result<Resolved<MusicReleaseGroup>, SdkError> {
        let search = self.search().await?;
        orchestrator::fetch_music(&search.fixer, &search.candidates, search.warnings).await
    }
}

/// Ranked music release-group search results.
pub struct MusicSearch {
    fixer: Fixer,
    candidates: Vec<Candidate>,
    warnings: Vec<ResolutionWarning>,
}

impl MusicSearch {
    /// Returns ranked typed music candidates.
    pub fn candidates(&self) -> &[Candidate] {
        &self.candidates
    }

    /// Returns non-fatal provider and matching warnings from the search.
    pub fn warnings(&self) -> &[ResolutionWarning] {
        &self.warnings
    }

    /// Selects one music candidate explicitly.
    pub fn select(self, index: usize) -> Result<SelectedMusic, SdkError> {
        let length = self.candidates.len();
        let candidate = self
            .candidates
            .into_iter()
            .nth(index)
            .ok_or(SdkError::CandidateOutOfBounds { index, length })?;
        Ok(SelectedMusic {
            fixer: self.fixer,
            candidate,
            warnings: self.warnings,
        })
    }
}

/// One explicit music candidate ready to fetch.
pub struct SelectedMusic {
    fixer: Fixer,
    candidate: Candidate,
    warnings: Vec<ResolutionWarning>,
}

impl SelectedMusic {
    /// Fetches only the explicitly selected music candidate.
    pub async fn fetch_selected(self) -> Result<Resolved<MusicReleaseGroup>, SdkError> {
        orchestrator::fetch_music(&self.fixer, &[self.candidate], self.warnings).await
    }
}
