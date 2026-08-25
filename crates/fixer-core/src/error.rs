//! Structured construction errors for core value objects.

use thiserror::Error;

/// Errors produced while constructing validated core values.
#[derive(Debug, Clone, PartialEq, Error)]
#[non_exhaustive]
pub enum CoreError {
    /// A language tag was not well-formed or valid BCP 47.
    #[error("invalid BCP 47 language tag `{input}`: {reason}")]
    InvalidLanguageTag { input: String, reason: String },
    /// A confidence value was not finite or outside the unit interval.
    #[error("confidence must be finite and between 0.0 and 1.0, got {value}")]
    InvalidConfidence { value: f32 },
    /// A provider identifier was empty or contained unsupported characters.
    #[error("invalid provider identifier `{input}`")]
    InvalidProviderId { input: String },
    /// An external identifier namespace or value was invalid.
    #[error("invalid external identifier {field}: `{input}`")]
    InvalidExternalId { field: &'static str, input: String },
    /// A domain value failed boundary validation.
    #[error("invalid {field}: `{value}`")]
    InvalidDomainValue { field: &'static str, value: String },
    /// A provenance field path was empty or malformed.
    #[error("invalid provenance field path `{input}`")]
    InvalidFieldPath { input: String },
}
