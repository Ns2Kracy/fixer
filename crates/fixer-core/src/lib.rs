//! Runtime-independent domain types and extension protocols for Fixer.
//!
//! The crate intentionally performs no filesystem or network I/O.

#![forbid(unsafe_code)]

mod confidence;
mod error;
pub mod http;
mod identity;
mod locale;
pub mod matching;
pub mod media;
pub mod merge;
pub mod output;
mod provenance;
pub mod provider;
mod resolved;

pub use confidence::Confidence;
pub use error::CoreError;
pub use http::{Header, HttpClient, HttpError, HttpMethod, HttpRequest, HttpResponse};
pub use identity::{ExternalId, ProviderId};
pub use locale::{LanguageTag, LocalePolicy, LocalizedEntry, LocalizedValue};
pub use matching::{
    MatchEvidence, MatchEvidenceKind, MatchQuery, MatchScore, MatchSelection, Matcher,
    MatchingError, RankedCandidate,
};
pub use media::*;
pub use merge::{FieldPath, MergeError, MergePolicy, MovieDocument, MovieMerger};
pub use output::{
    OutputOperation, OutputPlan, PlannedContent, PlanningError, WriteRequest, Writer,
};
pub use provenance::{ProvenanceMap, SourceRef, Sourced};
pub use provider::{
    BoxFuture, Candidate, FetchRequest, MediaKind, MetadataDocument, MovieCandidate, Provider,
    ProviderDescriptor, ProviderError, SearchRequest,
};
pub use resolved::{MergeConflict, ResolutionWarning, Resolved};
