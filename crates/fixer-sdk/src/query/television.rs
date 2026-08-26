//! Ergonomic typed television query flow.

use crate::{Fixer, SdkError, orchestrator};
use fixer_core::{Candidate, ExternalId, OrderingScheme, ResolutionWarning, Resolved, Series};

/// A typed television series query builder.
#[derive(Clone)]
pub struct TelevisionQuery {
    fixer: Fixer,
    title: String,
    year: Option<u16>,
    ordering: Option<OrderingScheme>,
    external_ids: Vec<ExternalId>,
}

impl TelevisionQuery {
    pub(crate) const fn new(fixer: Fixer, title: String) -> Self {
        Self {
            fixer,
            title,
            year: None,
            ordering: None,
            external_ids: Vec::new(),
        }
    }

    /// Restricts matching to a first-air year.
    pub const fn year(mut self, year: u16) -> Self {
        self.year = Some(year);
        self
    }

    /// Selects the ordering scheme retained by the resolved hierarchy.
    pub const fn ordering(mut self, ordering: OrderingScheme) -> Self {
        self.ordering = Some(ordering);
        self
    }

    /// Adds an exact provider external ID to matching evidence.
    pub fn external_id(mut self, external_id: ExternalId) -> Self {
        if !self.external_ids.contains(&external_id) {
            self.external_ids.push(external_id);
        }
        self
    }

    /// Searches providers and returns deterministic ranked candidates.
    pub async fn search(self) -> Result<TelevisionSearch, SdkError> {
        let outcome = orchestrator::search_television(
            &self.fixer,
            &self.title,
            self.year,
            &self.external_ids,
        )
        .await?;
        Ok(TelevisionSearch {
            fixer: self.fixer,
            candidates: outcome.candidates,
            warnings: outcome.warnings,
            ordering: self.ordering,
            external_ids: self.external_ids,
        })
    }

    /// Searches, fetches the deterministic candidate group, and merges its hierarchy.
    pub async fn resolve(self) -> Result<Resolved<Series>, SdkError> {
        let search = self.search().await?;
        orchestrator::fetch_series(
            &search.fixer,
            &search.candidates,
            search.warnings,
            search.ordering,
            &search.external_ids,
        )
        .await
    }
}

/// Ranked television search results.
pub struct TelevisionSearch {
    fixer: Fixer,
    candidates: Vec<Candidate>,
    warnings: Vec<ResolutionWarning>,
    ordering: Option<OrderingScheme>,
    external_ids: Vec<ExternalId>,
}

impl TelevisionSearch {
    /// Returns ranked candidates.
    pub fn candidates(&self) -> &[Candidate] {
        &self.candidates
    }

    /// Selects one candidate explicitly.
    pub fn select(self, index: usize) -> Result<SelectedTelevision, SdkError> {
        let length = self.candidates.len();
        let candidate = self
            .candidates
            .into_iter()
            .nth(index)
            .ok_or(SdkError::CandidateOutOfBounds { index, length })?;
        Ok(SelectedTelevision {
            fixer: self.fixer,
            candidate,
            warnings: self.warnings,
            ordering: self.ordering,
            external_ids: self.external_ids,
        })
    }
}

/// One explicit television candidate ready to fetch.
pub struct SelectedTelevision {
    fixer: Fixer,
    candidate: Candidate,
    warnings: Vec<ResolutionWarning>,
    ordering: Option<OrderingScheme>,
    external_ids: Vec<ExternalId>,
}

impl SelectedTelevision {
    /// Fetches and resolves the explicitly selected candidate.
    pub async fn fetch_selected(self) -> Result<Resolved<Series>, SdkError> {
        orchestrator::fetch_series(
            &self.fixer,
            &[self.candidate],
            self.warnings,
            self.ordering,
            &self.external_ids,
        )
        .await
    }
}
