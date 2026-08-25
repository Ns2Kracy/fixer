//! Runtime-independent domain types and extension protocols for Fixer.
//!
//! The crate intentionally performs no filesystem or network I/O.

#![forbid(unsafe_code)]

mod confidence;
mod error;
mod identity;
mod locale;
mod provenance;

pub use confidence::Confidence;
pub use error::CoreError;
pub use identity::{ExternalId, ProviderId};
pub use locale::{LanguageTag, LocalePolicy, LocalizedEntry, LocalizedValue};
pub use provenance::{ProvenanceMap, SourceRef, Sourced};
