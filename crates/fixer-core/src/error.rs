//! Structured construction errors for core value objects.

use thiserror::Error;

/// Errors produced while constructing validated core values.
#[derive(Debug, Clone, PartialEq, Error)]
#[non_exhaustive]
pub enum CoreError {
    /// A language tag was not well-formed or valid BCP 47.
    #[error("invalid BCP 47 language tag `{input}`: {reason}")]
    InvalidLanguageTag { input: String, reason: String },
    /// A provider identifier was empty or contained unsupported characters.
    #[error("invalid provider identifier `{input}`")]
    InvalidProviderId { input: String },
    /// An external identifier namespace or value was invalid.
    #[error("invalid external identifier {field}: `{input}`")]
    InvalidExternalId { field: &'static str, input: String },
    /// An exact provider target has an invalid media kind, namespace, or identifier.
    #[error("invalid exact target {provider}/{external_id} for media kind {media_kind:?}")]
    InvalidProviderTarget {
        provider: String,
        media_kind: crate::MediaKind,
        external_id: String,
    },
    /// A domain value failed boundary validation.
    #[error("invalid {field}: `{value}`")]
    InvalidDomainValue { field: &'static str, value: String },
    /// A provenance field path was empty or malformed.
    #[error("invalid provenance field path `{input}`")]
    InvalidFieldPath { input: String },
}
