//! Shared contracts for deterministic scrape selection and auditable output.

use crate::{CoreError, ExternalId, MediaKind, ProviderId};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

const MAX_AUDIT_PATH_BYTES: usize = 4096;

/// One exact record to fetch from a named metadata provider.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderTarget {
    media_kind: MediaKind,
    provider: ProviderId,
    external_id: ExternalId,
}

impl ProviderTarget {
    /// Creates a target whose external-ID namespace matches its provider.
    pub fn new(
        media_kind: MediaKind,
        provider: ProviderId,
        external_id: ExternalId,
    ) -> Result<Self, CoreError> {
        if external_id.namespace != provider.as_str() {
            return Err(CoreError::InvalidProviderTarget {
                provider: provider.to_string(),
                media_kind,
                external_id: external_id.value,
            });
        }
        Ok(Self {
            media_kind,
            provider,
            external_id,
        })
    }

    /// Creates a validated TMDB movie or television target.
    pub fn tmdb(media_kind: MediaKind, id: impl AsRef<str>) -> Result<Self, CoreError> {
        let id = id.as_ref();
        let supported_kind = matches!(media_kind, MediaKind::Movie | MediaKind::Television);
        let valid_id = !id.is_empty()
            && id.bytes().all(|byte| byte.is_ascii_digit())
            && id.bytes().any(|byte| byte != b'0');
        if !supported_kind || !valid_id {
            return Err(CoreError::InvalidProviderTarget {
                provider: "tmdb".to_owned(),
                media_kind,
                external_id: id.to_owned(),
            });
        }
        Self::new(
            media_kind,
            ProviderId::new("tmdb")?,
            ExternalId::new("tmdb", id)?,
        )
    }

    pub const fn media_kind(&self) -> MediaKind {
        self.media_kind
    }

    pub const fn provider(&self) -> &ProviderId {
        &self.provider
    }

    pub const fn external_id(&self) -> &ExternalId {
        &self.external_id
    }
}

/// Whether a scrape searches automatically or fetches one exact record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(tag = "mode", content = "target", rename_all = "snake_case")]
pub enum ScrapeSelection {
    #[default]
    Automatic,
    Exact(ProviderTarget),
}

/// Stable kind recorded for an attempted output operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutputOperationKind {
    CreateDirectory,
    WriteBytes,
    Copy,
    Move,
    Symlink,
    Hardlink,
    Reflink,
}

/// Stable outcome recorded for an attempted output operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationOutcome {
    DryRun,
    Succeeded,
    Failed,
}

/// Content fingerprint used to authorize replacement of a prior scrape output.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct OutputFingerprint(String);

impl OutputFingerprint {
    /// Accepts a SHA-256 digest encoded as 64 hexadecimal characters.
    pub fn new(value: impl Into<String>) -> Result<Self, CoreError> {
        let value = value.into();
        if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(CoreError::InvalidDomainValue {
                field: "output.fingerprint",
                value,
            });
        }
        Ok(Self(value.to_ascii_lowercase()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Durable audit entry for one attempted output operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperationReport {
    operation_index: u64,
    kind: OutputOperationKind,
    source: Option<String>,
    destination: String,
    outcome: OperationOutcome,
    fingerprint: Option<OutputFingerprint>,
}

impl OperationReport {
    pub fn new(
        operation_index: u64,
        kind: OutputOperationKind,
        source: Option<String>,
        destination: impl Into<String>,
        outcome: OperationOutcome,
        fingerprint: Option<OutputFingerprint>,
    ) -> Result<Self, CoreError> {
        let destination = validated_audit_path(destination.into(), "output.destination")?;
        let source = source
            .map(|value| validated_audit_path(value, "output.source"))
            .transpose()?;
        Ok(Self {
            operation_index,
            kind,
            source,
            destination,
            outcome,
            fingerprint,
        })
    }

    pub const fn operation_index(&self) -> u64 {
        self.operation_index
    }

    pub const fn kind(&self) -> OutputOperationKind {
        self.kind
    }

    pub fn source(&self) -> Option<&str> {
        self.source.as_deref()
    }

    pub fn destination(&self) -> &str {
        &self.destination
    }

    pub const fn outcome(&self) -> OperationOutcome {
        self.outcome
    }

    pub const fn fingerprint(&self) -> Option<&OutputFingerprint> {
        self.fingerprint.as_ref()
    }
}

/// Exact output fingerprints from a prior successful scrape.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReplacementManifest {
    outputs: BTreeMap<String, OutputFingerprint>,
}

impl ReplacementManifest {
    pub fn from_reports<'a>(
        reports: impl IntoIterator<Item = &'a OperationReport>,
    ) -> Result<Self, CoreError> {
        let mut outputs = BTreeMap::new();
        for report in reports {
            if report.outcome != OperationOutcome::Succeeded {
                continue;
            }
            let Some(fingerprint) = report.fingerprint.clone() else {
                continue;
            };
            if outputs
                .insert(report.destination.clone(), fingerprint.clone())
                .is_some_and(|previous| previous != fingerprint)
            {
                return Err(CoreError::InvalidDomainValue {
                    field: "replacement_manifest.destination",
                    value: report.destination.clone(),
                });
            }
        }
        Ok(Self { outputs })
    }

    /// Allows replacement only when both destination and current fingerprint match.
    pub fn allows(&self, destination: &str, current: &OutputFingerprint) -> bool {
        self.outputs.get(destination) == Some(current)
    }

    pub const fn outputs(&self) -> &BTreeMap<String, OutputFingerprint> {
        &self.outputs
    }
}

fn validated_audit_path(value: String, field: &'static str) -> Result<String, CoreError> {
    if value.is_empty()
        || value.len() > MAX_AUDIT_PATH_BYTES
        || value.chars().any(char::is_control)
    {
        return Err(CoreError::InvalidDomainValue { field, value });
    }
    Ok(value)
}
