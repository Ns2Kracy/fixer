//! Ergonomic typed book work and edition query flow.

use crate::{Fixer, SdkError, orchestrator};
use fixer_core::{BookWork, Candidate, ExternalId, Isbn13, ResolutionWarning, Resolved};

/// A typed book work query builder with optional exact edition evidence.
#[derive(Clone)]
pub struct BookQuery {
    fixer: Fixer,
    title: String,
    year: Option<u16>,
    isbn: Option<Isbn13>,
}

impl BookQuery {
    pub(crate) const fn new(fixer: Fixer, title: String) -> Self {
        Self {
            fixer,
            title,
            year: None,
            isbn: None,
        }
    }

    /// Restricts matching to a first-publication year.
    pub const fn year(mut self, year: u16) -> Self {
        self.year = Some(year);
        self
    }

    /// Adds an exact ISBN-13 edition identity to matching evidence.
    pub fn isbn(mut self, isbn: Isbn13) -> Self {
        self.isbn = Some(isbn);
        self
    }

    /// Searches providers and ranks exact ISBN editions above title-only matches.
    pub async fn search(self) -> Result<BookSearch, SdkError> {
        let external_id = self
            .isbn
            .as_ref()
            .map(|isbn| ExternalId::new("isbn", isbn.as_str()))
            .transpose()?;
        let outcome =
            orchestrator::search_book(&self.fixer, &self.title, self.year, external_id.as_ref())
                .await?;
        Ok(BookSearch {
            fixer: self.fixer,
            candidates: outcome.candidates,
            warnings: outcome.warnings,
        })
    }

    /// Searches and fetches only the deterministic top edition candidate.
    pub async fn resolve(self) -> Result<Resolved<BookWork>, SdkError> {
        let search = self.search().await?;
        orchestrator::fetch_book(&search.fixer, &search.candidates, search.warnings).await
    }
}

/// Ranked book edition search results.
pub struct BookSearch {
    fixer: Fixer,
    candidates: Vec<Candidate>,
    warnings: Vec<ResolutionWarning>,
}

impl BookSearch {
    /// Returns ranked typed book candidates.
    pub fn candidates(&self) -> &[Candidate] {
        &self.candidates
    }

    /// Selects one book edition candidate explicitly.
    pub fn select(self, index: usize) -> Result<SelectedBook, SdkError> {
        let length = self.candidates.len();
        let candidate = self
            .candidates
            .into_iter()
            .nth(index)
            .ok_or(SdkError::CandidateOutOfBounds { index, length })?;
        Ok(SelectedBook {
            fixer: self.fixer,
            candidate,
            warnings: self.warnings,
        })
    }
}

/// One explicit book edition candidate ready to fetch.
pub struct SelectedBook {
    fixer: Fixer,
    candidate: Candidate,
    warnings: Vec<ResolutionWarning>,
}

impl SelectedBook {
    /// Fetches only the explicitly selected edition candidate.
    pub async fn fetch_selected(self) -> Result<Resolved<BookWork>, SdkError> {
        orchestrator::fetch_book(&self.fixer, &[self.candidate], self.warnings).await
    }
}
