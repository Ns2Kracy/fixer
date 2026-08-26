//! Ergonomic typed movie query flow.

use crate::{Fixer, SdkError, orchestrator};
use fixer_core::{Candidate, Movie, ResolutionWarning, Resolved};

/// A typed movie query builder.
#[derive(Clone)]
pub struct MovieQuery {
    fixer: Fixer,
    title: String,
    year: Option<u16>,
}
impl MovieQuery {
    pub(crate) const fn new(fixer: Fixer, title: String) -> Self {
        Self {
            fixer,
            title,
            year: None,
        }
    }
    /// Restricts matching to a release year.
    pub const fn year(mut self, year: u16) -> Self {
        self.year = Some(year);
        self
    }
    /// Searches providers and returns deterministic ranked candidates.
    pub async fn search(self) -> Result<MovieSearch, SdkError> {
        let outcome = orchestrator::search_movie(&self.fixer, &self.title, self.year).await?;
        Ok(MovieSearch {
            fixer: self.fixer,
            candidates: outcome.candidates,
            warnings: outcome.warnings,
        })
    }
    /// Searches, fetches the deterministic top candidate group, and merges metadata.
    pub async fn resolve(self) -> Result<Resolved<Movie>, SdkError> {
        let search = self.search().await?;
        orchestrator::fetch_movies(&search.fixer, &search.candidates, search.warnings).await
    }
}

/// Ranked movie search results.
pub struct MovieSearch {
    fixer: Fixer,
    candidates: Vec<Candidate>,
    warnings: Vec<ResolutionWarning>,
}
impl MovieSearch {
    /// Returns ranked candidates.
    pub fn candidates(&self) -> &[Candidate] {
        &self.candidates
    }
    /// Returns non-fatal provider and matching warnings from the search.
    pub fn warnings(&self) -> &[ResolutionWarning] {
        &self.warnings
    }

    /// Selects one candidate explicitly.
    pub fn select(self, index: usize) -> Result<SelectedMovie, SdkError> {
        let length = self.candidates.len();
        let candidate = self
            .candidates
            .into_iter()
            .nth(index)
            .ok_or(SdkError::CandidateOutOfBounds { index, length })?;
        Ok(SelectedMovie {
            fixer: self.fixer,
            candidate,
            warnings: self.warnings,
        })
    }
}

/// One explicit candidate ready to fetch.
pub struct SelectedMovie {
    fixer: Fixer,
    candidate: Candidate,
    warnings: Vec<ResolutionWarning>,
}
impl SelectedMovie {
    /// Fetches and resolves the explicitly selected candidate.
    pub async fn fetch_selected(self) -> Result<Resolved<Movie>, SdkError> {
        orchestrator::fetch_movies(&self.fixer, &[self.candidate], self.warnings).await
    }
}
