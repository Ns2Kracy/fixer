//! Provider precedence policy for metadata merging.

use crate::{CoreError, MediaKind, ProviderId};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// A validated dotted metadata field path.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct FieldPath(String);

impl FieldPath {
    /// Constructs a field path such as `movie.summaries`.
    pub fn new(value: impl Into<String>) -> Result<Self, CoreError> {
        let value = value.into();
        let valid = !value.is_empty()
            && value.split('.').all(|segment| {
                !segment.is_empty()
                    && segment
                        .chars()
                        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-'))
            });
        if valid {
            Ok(Self(value))
        } else {
            Err(CoreError::InvalidFieldPath { input: value })
        }
    }
    /// Returns the dotted field path.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Provider order with global, media-kind, and field-path overrides.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MergePolicy {
    global_order: Vec<ProviderId>,
    media_order: BTreeMap<MediaKind, Vec<ProviderId>>,
    field_order: BTreeMap<FieldPath, Vec<ProviderId>>,
}

impl MergePolicy {
    /// Constructs a policy with global provider order.
    pub fn new(order: impl IntoIterator<Item = ProviderId>) -> Self {
        Self {
            global_order: deduplicate(order),
            media_order: BTreeMap::new(),
            field_order: BTreeMap::new(),
        }
    }
    /// Adds or replaces precedence for a media kind.
    pub fn with_media_order(
        mut self,
        media_kind: MediaKind,
        order: impl IntoIterator<Item = ProviderId>,
    ) -> Self {
        self.media_order.insert(media_kind, deduplicate(order));
        self
    }
    /// Adds or replaces precedence for a field path.
    pub fn with_field_order(
        mut self,
        field_path: FieldPath,
        order: impl IntoIterator<Item = ProviderId>,
    ) -> Self {
        self.field_order.insert(field_path, deduplicate(order));
        self
    }
    /// Returns effective precedence: field, then media, then global.
    pub fn order_for(&self, media_kind: MediaKind, field_path: &FieldPath) -> &[ProviderId] {
        self.field_order
            .get(field_path)
            .or_else(|| self.media_order.get(&media_kind))
            .map_or(&self.global_order, Vec::as_slice)
    }
    pub(crate) fn rank(
        &self,
        media_kind: MediaKind,
        field_path: &FieldPath,
        provider: &ProviderId,
    ) -> usize {
        self.order_for(media_kind, field_path)
            .iter()
            .position(|item| item == provider)
            .unwrap_or(usize::MAX)
    }
}

fn deduplicate(order: impl IntoIterator<Item = ProviderId>) -> Vec<ProviderId> {
    let mut result = Vec::new();
    for provider in order {
        if !result.contains(&provider) {
            result.push(provider);
        }
    }
    result
}
