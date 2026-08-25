//! Source references and field-level provenance.

use crate::{Confidence, CoreError, ExternalId, LanguageTag, ProviderId};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    time::{SystemTime, UNIX_EPOCH},
};

/// The provider observation that supplied a metadata value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceRef {
    /// Provider that observed the value.
    pub provider: ProviderId,
    /// Provider-specific identifier, when one is available.
    pub external_id: Option<ExternalId>,
    /// Language associated with the observation, when known.
    pub locale: Option<LanguageTag>,
    /// Observation time as Unix epoch milliseconds.
    pub observed_at_unix_ms: u64,
}

impl SourceRef {
    /// Constructs a source reference from a system time.
    pub fn new(
        provider: ProviderId,
        external_id: Option<ExternalId>,
        locale: Option<LanguageTag>,
        observed_at: SystemTime,
    ) -> Self {
        let millis = observed_at
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        Self {
            provider,
            external_id,
            locale,
            observed_at_unix_ms: u64::try_from(millis).unwrap_or(u64::MAX),
        }
    }

    /// Constructs a local observation at the current time.
    pub fn local(provider: ProviderId) -> Self {
        Self::new(provider, None, None, SystemTime::now())
    }
}

/// A metadata value together with its source and confidence.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Sourced<T> {
    /// Observed value.
    pub value: T,
    /// Source that supplied the value.
    pub source: SourceRef,
    /// Confidence assigned to the observation.
    pub confidence: Confidence,
}

/// Field path to one or more contributing sources.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ProvenanceMap(BTreeMap<String, Vec<SourceRef>>);

impl ProvenanceMap {
    /// Creates an empty provenance map.
    pub const fn new() -> Self {
        Self(BTreeMap::new())
    }

    /// Adds a source for a non-empty field path.
    pub fn add(
        &mut self,
        field_path: impl Into<String>,
        source: SourceRef,
    ) -> Result<(), CoreError> {
        let field_path = field_path.into();
        let valid = !field_path.is_empty()
            && field_path.split('.').all(|segment| {
                !segment.is_empty()
                    && segment
                        .chars()
                        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-'))
            });
        if !valid {
            return Err(CoreError::InvalidFieldPath { input: field_path });
        }
        self.0.entry(field_path).or_default().push(source);
        Ok(())
    }

    /// Returns all sources recorded for a field path.
    pub fn sources_for(&self, field_path: &str) -> &[SourceRef] {
        self.0
            .get(field_path)
            .map(Vec::as_slice)
            .unwrap_or_default()
    }
}
