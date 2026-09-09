//! Selection contracts shared by scrape orchestrators.

use crate::{CoreError, ExternalId, MediaKind, ProviderId};
use serde::{Deserialize, Serialize};

/// An exact provider record to fetch without running a search.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderTarget {
    media_kind: MediaKind,
    provider: ProviderId,
    external_id: ExternalId,
}

impl ProviderTarget {
    /// Constructs a target from already validated domain identifiers.
    pub const fn new(media_kind: MediaKind, provider: ProviderId, external_id: ExternalId) -> Self {
        Self {
            media_kind,
            provider,
            external_id,
        }
    }

    /// Constructs an exact TMDB movie or television target.
    pub fn tmdb(media_kind: MediaKind, id: impl Into<String>) -> Result<Self, CoreError> {
        if !matches!(media_kind, MediaKind::Movie | MediaKind::Television) {
            return Err(CoreError::InvalidDomainValue {
                field: "scrape.target.media_kind",
                value: format!("{media_kind:?}"),
            });
        }

        let id = id.into();
        if id.is_empty() || !id.bytes().all(|byte| byte.is_ascii_digit()) || id == "0" {
            return Err(CoreError::InvalidDomainValue {
                field: "scrape.target.external_id",
                value: id,
            });
        }

        Ok(Self::new(
            media_kind,
            ProviderId::new("tmdb")?,
            ExternalId::new("tmdb", id)?,
        ))
    }

    /// Returns the target media domain.
    pub const fn media_kind(&self) -> MediaKind {
        self.media_kind
    }

    /// Returns the provider that owns the target.
    pub const fn provider(&self) -> &ProviderId {
        &self.provider
    }

    /// Returns the provider-specific target ID.
    pub const fn external_id(&self) -> &ExternalId {
        &self.external_id
    }
}

/// How a scrape chooses remote metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode", content = "target", rename_all = "snake_case")]
pub enum ScrapeSelection {
    /// Search configured providers and use the first deterministic result.
    Automatic,
    /// Skip search and fetch one exact provider record.
    Exact(ProviderTarget),
}
