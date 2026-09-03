//! Deterministic in-memory provider for tests, examples, and offline flows.

use fixer_core::{
    BookCandidate, BoxFuture, Candidate, ExternalId, FetchRequest, HttpClient, MediaKind,
    MetadataDocument, MovieCandidate, Provider, ProviderDescriptor, ProviderError, ProviderId,
    SearchRequest, TelevisionCandidate,
};
use std::time::Duration;

/// One in-memory provider document and its fetch identity.
#[derive(Debug, Clone)]
pub struct FixtureDocument {
    /// External ID used by search and fetch.
    pub external_id: ExternalId,
    /// Typed metadata returned by fetch.
    pub document: MetadataDocument,
}

impl FixtureDocument {
    /// Constructs a fixture document.
    pub const fn new(external_id: ExternalId, document: MetadataDocument) -> Self {
        Self {
            external_id,
            document,
        }
    }
}

/// A deterministic local provider backed by typed fixture documents.
#[derive(Debug, Clone)]
pub struct FixtureProvider {
    descriptor: ProviderDescriptor,
    documents: Vec<FixtureDocument>,
    search_delay: Duration,
}

impl FixtureProvider {
    /// Constructs a network-free fixture provider.
    pub fn new(
        id: ProviderId,
        documents: impl IntoIterator<Item = FixtureDocument>,
    ) -> Result<Self, fixer_core::CoreError> {
        let documents = documents.into_iter().collect::<Vec<_>>();
        let media_kinds = documents
            .iter()
            .map(|document| document.document.media_kind())
            .collect::<std::collections::BTreeSet<_>>();
        let media_kinds = if media_kinds.is_empty() {
            std::iter::once(MediaKind::Movie).collect()
        } else {
            media_kinds
        };
        let descriptor =
            ProviderDescriptor::new(id, "Fixture", media_kinds)?.with_network_requirement(false);
        Ok(Self {
            descriptor,
            documents,
            search_delay: Duration::ZERO,
        })
    }

    /// Adds an artificial search delay for concurrency tests.
    pub const fn with_search_delay(mut self, delay: Duration) -> Self {
        self.search_delay = delay;
        self
    }
}

impl Provider for FixtureProvider {
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
            if !self.search_delay.is_zero() {
                tokio::time::sleep(self.search_delay).await;
            }
            Ok(match request {
                SearchRequest::Movie { title, year, .. } => self
                    .documents
                    .iter()
                    .filter_map(|fixture| {
                        let MetadataDocument::Movie(movie) = &fixture.document else {
                            return None;
                        };
                        let candidate_title = movie
                            .titles
                            .entries()
                            .first()
                            .map_or_else(|| title.clone(), |entry| entry.value().clone());
                        let candidate_year = movie.release_year().or(year);
                        MovieCandidate::new(
                            self.descriptor.id().clone(),
                            fixture.external_id.clone(),
                            candidate_title,
                            candidate_year,
                        )
                        .ok()
                        .map(Candidate::Movie)
                    })
                    .collect::<Vec<_>>(),
                SearchRequest::Book { title, year, .. } => self
                    .documents
                    .iter()
                    .filter_map(|fixture| {
                        let MetadataDocument::Book(book) = &fixture.document else {
                            return None;
                        };
                        let candidate_title = book
                            .titles
                            .entries()
                            .first()
                            .map_or_else(|| title.clone(), |entry| entry.value().clone());
                        BookCandidate::new(
                            self.descriptor.id().clone(),
                            fixture.external_id.clone(),
                            candidate_title,
                            year,
                        )
                        .ok()
                        .map(Candidate::Book)
                    })
                    .collect::<Vec<_>>(),
                SearchRequest::Television { title, year, .. } => self
                    .documents
                    .iter()
                    .filter_map(|fixture| {
                        let MetadataDocument::Television(series) = &fixture.document else {
                            return None;
                        };
                        let candidate_title = series
                            .titles
                            .entries()
                            .first()
                            .map_or_else(|| title.clone(), |entry| entry.value().clone());
                        TelevisionCandidate::new(
                            self.descriptor.id().clone(),
                            fixture.external_id.clone(),
                            candidate_title,
                            year,
                        )
                        .ok()
                        .map(Candidate::Television)
                    })
                    .collect::<Vec<_>>(),
                _ => {
                    return Err(ProviderError::UnsupportedMedia {
                        provider: self.descriptor.id().clone(),
                        media_kind,
                    });
                }
            })
        })
    }

    fn fetch<'a>(
        &'a self,
        request: FetchRequest,
        _http: &'a dyn HttpClient,
    ) -> BoxFuture<'a, Result<MetadataDocument, ProviderError>> {
        Box::pin(async move {
            self.descriptor.ensure_support(request.media_kind())?;
            self.documents
                .iter()
                .find(|fixture| {
                    fixture.external_id == request.external_id
                        && fixture.document.media_kind() == request.media_kind()
                })
                .map(|fixture| fixture.document.clone())
                .ok_or(ProviderError::NotFound)
        })
    }
}
