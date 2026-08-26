//! Metadata merge policies and typed merge entry points.

mod policy;
mod television;

pub use policy::{FieldPath, MergePolicy};
pub use television::{SeriesDocument, SeriesMerger};

use crate::{
    ArtworkReference, ContentRating, CoreError, Credit, Genre, LocalizedEntry, LocalizedValue,
    MediaKind, MergeConflict, MetadataDocument, Movie, MovieRelease, ProvenanceMap, ProviderId,
    Rating, ResolutionWarning, Resolved, SourceRef,
};
use thiserror::Error;

/// A movie document paired with provider source metadata.
#[derive(Debug, Clone, PartialEq)]
pub struct MovieDocument {
    pub value: Movie,
    pub source: SourceRef,
}
impl MovieDocument {
    pub const fn new(value: Movie, source: SourceRef) -> Self {
        Self { value, source }
    }
}

/// Explicit merge failures.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum MergeError {
    #[error("no metadata documents were supplied")]
    NoDocuments,
    #[error("metadata document lacks source metadata")]
    MissingSource,
    #[error("unsupported metadata document: {0:?}")]
    UnsupportedDocument(MediaKind),
    #[error("television ordering mismatch: expected {expected:?}, found {found:?}")]
    OrderingMismatch {
        expected: crate::OrderingScheme,
        found: crate::OrderingScheme,
    },
    #[error("invalid merge data: {0}")]
    Invalid(String),
}
impl From<CoreError> for MergeError {
    fn from(error: CoreError) -> Self {
        Self::Invalid(error.to_string())
    }
}

/// Deterministic typed movie merger.
#[derive(Debug, Clone)]
pub struct MovieMerger {
    policy: MergePolicy,
}
impl MovieMerger {
    /// Constructs a merger with explicit provider precedence.
    pub const fn new(policy: MergePolicy) -> Self {
        Self { policy }
    }

    /// Merges movie documents with field-level provenance.
    pub fn merge(
        &self,
        documents: impl IntoIterator<Item = MovieDocument>,
    ) -> Result<Resolved<Movie>, MergeError> {
        let mut documents = documents.into_iter().collect::<Vec<_>>();
        if documents.is_empty() {
            return Err(MergeError::NoDocuments);
        }
        let base_path = FieldPath::new("movie")?;
        documents.sort_by_key(|document| {
            self.policy
                .rank(MediaKind::Movie, &base_path, &document.source.provider)
        });
        let mut merged = documents[0].value.clone();
        let mut provenance = ProvenanceMap::new();
        let mut conflicts = Vec::new();

        merged.titles = merge_localized(
            "movie.titles",
            &documents,
            |movie| &movie.titles,
            &self.policy,
            &mut provenance,
            &mut conflicts,
        )?;
        merged.summaries = merge_localized(
            "movie.summaries",
            &documents,
            |movie| &movie.summaries,
            &self.policy,
            &mut provenance,
            &mut conflicts,
        )?;
        merged.releases = merge_unique(
            ordered_documents(&documents, "movie.releases", &self.policy)?.as_slice(),
            |movie| &movie.releases,
            |value: &MovieRelease| value.id.as_str().to_owned(),
            "movie.releases",
            &mut provenance,
        )?;
        merged.credits = merge_credits(
            ordered_documents(&documents, "movie.credits", &self.policy)?.as_slice(),
            &mut provenance,
        )?;
        merged.genres = merge_unique(
            ordered_documents(&documents, "movie.genres", &self.policy)?.as_slice(),
            |movie| &movie.genres,
            |value: &Genre| normalize(value.as_str()),
            "movie.genres",
            &mut provenance,
        )?;
        merged.artwork = merge_unique(
            ordered_documents(&documents, "movie.artwork", &self.policy)?.as_slice(),
            |movie| &movie.artwork,
            artwork_key,
            "movie.artwork",
            &mut provenance,
        )?;
        merged.ratings = merge_unique(
            ordered_documents(&documents, "movie.ratings", &self.policy)?.as_slice(),
            |movie| &movie.ratings,
            rating_key,
            "movie.ratings",
            &mut provenance,
        )?;
        merged.content_ratings = merge_unique(
            ordered_documents(&documents, "movie.content_ratings", &self.policy)?.as_slice(),
            |movie| &movie.content_ratings,
            content_rating_key,
            "movie.content_ratings",
            &mut provenance,
        )?;

        let completeness = completeness(&merged);
        let warnings = if completeness < 1.0 {
            vec![ResolutionWarning {
                code: "incomplete_metadata".to_owned(),
                message: format!("movie metadata is {:.0}% complete", completeness * 100.0),
            }]
        } else {
            Vec::new()
        };
        Ok(Resolved {
            value: merged,
            provenance,
            conflicts,
            completeness,
            warnings,
        })
    }

    /// Rejects bare heterogeneous documents because field provenance is required.
    pub fn merge_documents(
        &self,
        documents: impl IntoIterator<Item = MetadataDocument>,
    ) -> Result<Resolved<Movie>, MergeError> {
        let mut saw_movie = false;
        for document in documents {
            match document {
                MetadataDocument::Movie(_) => saw_movie = true,
                other => return Err(MergeError::UnsupportedDocument(other.media_kind())),
            }
        }
        if saw_movie {
            Err(MergeError::MissingSource)
        } else {
            Err(MergeError::NoDocuments)
        }
    }
}

fn merge_localized(
    path: &str,
    documents: &[MovieDocument],
    select: impl Fn(&Movie) -> &LocalizedValue<String>,
    policy: &MergePolicy,
    provenance: &mut ProvenanceMap,
    conflicts: &mut Vec<MergeConflict>,
) -> Result<LocalizedValue<String>, MergeError> {
    let field_path = FieldPath::new(path)?;
    let mut ordered = documents.iter().collect::<Vec<_>>();
    ordered.sort_by_key(|document| {
        policy.rank(MediaKind::Movie, &field_path, &document.source.provider)
    });
    let mut result = LocalizedValue::new();
    let mut identities = std::collections::BTreeMap::<String, (String, ProviderId)>::new();
    for document in ordered {
        for entry in select(&document.value).entries() {
            let language = entry
                .language()
                .map(|tag| tag.normalized().to_owned())
                .unwrap_or_else(|| "<untagged>".to_owned());
            let value = entry.value();
            if let Some((existing, provider)) = identities.get(&language) {
                if normalize(existing) != normalize(value) {
                    conflicts.push(MergeConflict {
                        field_path: format!("{path}.{language}"),
                        providers: vec![provider.clone(), document.source.provider.clone()],
                        message: "providers supplied different localized values".to_owned(),
                    });
                }
                continue;
            }
            identities.insert(language, (value.clone(), document.source.provider.clone()));
            match entry {
                LocalizedEntry::Tagged { language, value } => {
                    result.insert(language.as_str(), value.clone())?
                }
                LocalizedEntry::Untagged { value } => result.insert_untagged(value.clone()),
            }
            provenance.add(path, document.source.clone())?;
        }
    }
    Ok(result)
}

fn ordered_documents<'a>(
    documents: &'a [MovieDocument],
    path: &str,
    policy: &MergePolicy,
) -> Result<Vec<&'a MovieDocument>, MergeError> {
    let field_path = FieldPath::new(path)?;
    let mut ordered = documents.iter().collect::<Vec<_>>();
    ordered.sort_by_key(|document| {
        policy.rank(MediaKind::Movie, &field_path, &document.source.provider)
    });
    Ok(ordered)
}

fn merge_unique<T: Clone>(
    documents: &[&MovieDocument],
    select: impl Fn(&Movie) -> &[T],
    key: impl Fn(&T) -> String,
    path: &str,
    provenance: &mut ProvenanceMap,
) -> Result<Vec<T>, MergeError> {
    let mut seen = std::collections::BTreeSet::new();
    let mut result = Vec::new();
    for document in documents {
        for value in select(&document.value) {
            if seen.insert(key(value)) {
                result.push(value.clone());
                provenance.add(path, document.source.clone())?;
            }
        }
    }
    Ok(result)
}

fn merge_credits(
    documents: &[&MovieDocument],
    provenance: &mut ProvenanceMap,
) -> Result<Vec<Credit>, MergeError> {
    let mut ids = std::collections::BTreeSet::new();
    let mut identities = std::collections::BTreeSet::new();
    let mut result = Vec::new();
    for document in documents {
        for credit in &document.value.credits {
            let id = credit.person.id.as_str().to_owned();
            let identity = format!("{}:{:?}", normalize(&credit.person.name), credit.role);
            if ids.contains(&id) || identities.contains(&identity) {
                continue;
            }
            ids.insert(id);
            identities.insert(identity);
            result.push(credit.clone());
            provenance.add("movie.credits", document.source.clone())?;
        }
    }
    Ok(result)
}

fn artwork_key(value: &ArtworkReference) -> String {
    value.external_id.as_ref().map_or_else(
        || format!("identity:{:?}:{}", value.kind, normalize(&value.location)),
        |id| format!("id:{}:{}", id.namespace, id.value),
    )
}
fn rating_key(value: &Rating) -> String {
    normalize(&value.system)
}
fn content_rating_key(value: &ContentRating) -> String {
    format!("{}:{}", normalize(&value.system), normalize(&value.value))
}
fn normalize(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}
fn completeness(movie: &Movie) -> f32 {
    let present = [
        !movie.titles.entries().is_empty(),
        !movie.summaries.entries().is_empty(),
        !movie.releases.is_empty(),
        !movie.credits.is_empty(),
        !movie.genres.is_empty(),
        !movie.artwork.is_empty(),
    ]
    .into_iter()
    .filter(|present| *present)
    .count();
    present as f32 / 6.0
}
