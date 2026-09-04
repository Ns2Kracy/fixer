use std::{fmt, num::NonZeroI64};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{jobs::model::JobMediaKind, store::JobId};

const MAX_NAME_BYTES: usize = 100;
const MAX_ROOT_ID_BYTES: usize = 128;
const MAX_RELATIVE_PATH_BYTES: usize = 4096;
const MAX_TEMPLATE_BYTES: usize = 4096;
const MAX_LAST_ERROR_BYTES: usize = 1000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaKindMode {
    Auto,
    Fixed(JobMediaKind),
}

impl MediaKindMode {
    pub(crate) const fn storage_parts(self) -> (&'static str, Option<&'static str>) {
        match self {
            Self::Auto => ("auto", None),
            Self::Fixed(kind) => ("fixed", Some(media_kind_name(kind))),
        }
    }

    pub(crate) fn from_storage(
        mode: &str,
        fixed_kind: Option<&str>,
    ) -> Result<Self, IngestionModelError> {
        match (mode, fixed_kind) {
            ("auto", None) => Ok(Self::Auto),
            ("fixed", Some(kind)) => Ok(Self::Fixed(parse_media_kind(kind)?)),
            _ => Err(IngestionModelError::InvalidMediaKindMode),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RulePlacement {
    Move,
    Copy,
    Hardlink,
    Symlink,
    Reflink,
}

impl RulePlacement {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Move => "move",
            Self::Copy => "copy",
            Self::Hardlink => "hardlink",
            Self::Symlink => "symlink",
            Self::Reflink => "reflink",
        }
    }

    pub(crate) fn from_storage(value: &str) -> Result<Self, IngestionModelError> {
        match value {
            "move" => Ok(Self::Move),
            "copy" => Ok(Self::Copy),
            "hardlink" => Ok(Self::Hardlink),
            "symlink" => Ok(Self::Symlink),
            "reflink" => Ok(Self::Reflink),
            _ => Err(IngestionModelError::InvalidPlacement),
        }
    }
}

impl fmt::Display for RulePlacement {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuleStatus {
    Watching,
    Processing,
    NeedsReview,
    Paused,
    Error,
}

impl RuleStatus {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Watching => "watching",
            Self::Processing => "processing",
            Self::NeedsReview => "needs_review",
            Self::Paused => "paused",
            Self::Error => "error",
        }
    }

    pub(crate) fn from_storage(value: &str) -> Result<Self, IngestionModelError> {
        match value {
            "watching" => Ok(Self::Watching),
            "processing" => Ok(Self::Processing),
            "needs_review" => Ok(Self::NeedsReview),
            "paused" => Ok(Self::Paused),
            "error" => Ok(Self::Error),
            _ => Err(IngestionModelError::InvalidStatus),
        }
    }
}

impl fmt::Display for RuleStatus {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleDirectory {
    root_id: String,
    relative_path: String,
}

impl RuleDirectory {
    pub fn new(
        root_id: impl Into<String>,
        relative_path: impl Into<String>,
    ) -> Result<Self, IngestionModelError> {
        let root_id = root_id.into();
        let relative_path = relative_path.into();
        validate_required("root id", &root_id, MAX_ROOT_ID_BYTES)?;
        validate_optional("relative path", &relative_path, MAX_RELATIVE_PATH_BYTES)?;
        Ok(Self {
            root_id,
            relative_path,
        })
    }

    pub fn root_id(&self) -> &str {
        &self.root_id
    }

    pub fn relative_path(&self) -> &str {
        &self.relative_path
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IngestionRuleInput {
    name: String,
    source: RuleDirectory,
    destination: RuleDirectory,
    media_kind_mode: MediaKindMode,
    placement: RulePlacement,
    path_template_override: Option<String>,
    enabled: bool,
    last_error: Option<String>,
}

impl IngestionRuleInput {
    pub fn new(
        name: impl Into<String>,
        source: RuleDirectory,
        destination: RuleDirectory,
        media_kind_mode: MediaKindMode,
        placement: RulePlacement,
    ) -> Result<Self, IngestionModelError> {
        let name = name.into().trim().to_owned();
        validate_required("rule name", &name, MAX_NAME_BYTES)?;
        Ok(Self {
            name,
            source,
            destination,
            media_kind_mode,
            placement,
            path_template_override: None,
            enabled: true,
            last_error: None,
        })
    }

    pub fn with_path_template_override(
        mut self,
        value: impl Into<String>,
    ) -> Result<Self, IngestionModelError> {
        let value = value.into();
        validate_required("path template override", &value, MAX_TEMPLATE_BYTES)?;
        self.path_template_override = Some(value);
        Ok(self)
    }

    pub const fn with_enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    pub fn with_last_error(
        mut self,
        value: impl Into<String>,
    ) -> Result<Self, IngestionModelError> {
        let value = value.into();
        validate_required("last error", &value, MAX_LAST_ERROR_BYTES)?;
        self.last_error = Some(value);
        Ok(self)
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub const fn source(&self) -> &RuleDirectory {
        &self.source
    }

    pub const fn destination(&self) -> &RuleDirectory {
        &self.destination
    }

    pub const fn media_kind_mode(&self) -> MediaKindMode {
        self.media_kind_mode
    }

    pub const fn placement(&self) -> RulePlacement {
        self.placement
    }

    pub fn path_template_override(&self) -> Option<&str> {
        self.path_template_override.as_deref()
    }

    pub const fn enabled(&self) -> bool {
        self.enabled
    }

    pub fn last_error(&self) -> Option<&str> {
        self.last_error.as_deref()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct IngestionRuleId(NonZeroI64);

impl IngestionRuleId {
    pub const fn get(self) -> i64 {
        self.0.get()
    }

    pub(crate) fn from_database(value: i64) -> Result<Self, IngestionModelError> {
        NonZeroI64::new(value)
            .filter(|value| value.get() > 0)
            .map(Self)
            .ok_or(IngestionModelError::InvalidRuleId)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IngestionRule {
    id: IngestionRuleId,
    input: IngestionRuleInput,
    created_at_ms: i64,
    updated_at_ms: i64,
}

impl IngestionRule {
    pub const fn id(&self) -> IngestionRuleId {
        self.id
    }

    pub fn name(&self) -> &str {
        self.input.name()
    }

    pub const fn source(&self) -> &RuleDirectory {
        self.input.source()
    }

    pub const fn destination(&self) -> &RuleDirectory {
        self.input.destination()
    }

    pub const fn media_kind_mode(&self) -> MediaKindMode {
        self.input.media_kind_mode()
    }

    pub const fn placement(&self) -> RulePlacement {
        self.input.placement()
    }

    pub fn path_template_override(&self) -> Option<&str> {
        self.input.path_template_override()
    }

    pub const fn enabled(&self) -> bool {
        self.input.enabled()
    }

    pub fn last_error(&self) -> Option<&str> {
        self.input.last_error()
    }

    pub const fn created_at_ms(&self) -> i64 {
        self.created_at_ms
    }

    pub const fn updated_at_ms(&self) -> i64 {
        self.updated_at_ms
    }

    pub(crate) fn from_parts(parts: IngestionRuleParts) -> Self {
        Self {
            id: parts.id,
            input: parts.input,
            created_at_ms: parts.created_at_ms,
            updated_at_ms: parts.updated_at_ms,
        }
    }
}

pub(crate) struct IngestionRuleParts {
    pub id: IngestionRuleId,
    pub input: IngestionRuleInput,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceFingerprint {
    relative_source_path: String,
    size_bytes: u64,
    modified_at_ms: i64,
}

impl SourceFingerprint {
    pub fn new(
        relative_source_path: impl Into<String>,
        size_bytes: u64,
        modified_at_ms: i64,
    ) -> Result<Self, IngestionModelError> {
        let relative_source_path = relative_source_path.into();
        validate_required(
            "relative source path",
            &relative_source_path,
            MAX_RELATIVE_PATH_BYTES,
        )?;
        if size_bytes > i64::MAX as u64 {
            return Err(IngestionModelError::SourceSizeOutOfRange);
        }
        if modified_at_ms < 0 {
            return Err(IngestionModelError::ModifiedTimeOutOfRange);
        }
        Ok(Self {
            relative_source_path,
            size_bytes,
            modified_at_ms,
        })
    }

    pub fn relative_source_path(&self) -> &str {
        &self.relative_source_path
    }

    pub const fn size_bytes(&self) -> u64 {
        self.size_bytes
    }

    pub const fn modified_at_ms(&self) -> i64 {
        self.modified_at_ms
    }

    pub(crate) fn size_for_database(&self) -> i64 {
        i64::try_from(self.size_bytes).expect("source size is validated during construction")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct IngestionSourceId(NonZeroI64);

impl IngestionSourceId {
    pub const fn get(self) -> i64 {
        self.0.get()
    }

    pub(crate) fn from_database(value: i64) -> Result<Self, IngestionModelError> {
        NonZeroI64::new(value)
            .filter(|value| value.get() > 0)
            .map(Self)
            .ok_or(IngestionModelError::InvalidSourceId)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IngestionSourceReview {
    source_id: IngestionSourceId,
    rule_id: IngestionRuleId,
    relative_source_path: String,
    media_kinds: Vec<JobMediaKind>,
}

impl IngestionSourceReview {
    pub(crate) const fn new(
        source_id: IngestionSourceId,
        rule_id: IngestionRuleId,
        relative_source_path: String,
        media_kinds: Vec<JobMediaKind>,
    ) -> Self {
        Self {
            source_id,
            rule_id,
            relative_source_path,
            media_kinds,
        }
    }

    pub const fn source_id(&self) -> IngestionSourceId {
        self.source_id
    }

    pub const fn rule_id(&self) -> IngestionRuleId {
        self.rule_id
    }

    pub fn relative_source_path(&self) -> &str {
        &self.relative_source_path
    }

    pub fn media_kinds(&self) -> &[JobMediaKind] {
        &self.media_kinds
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IngestionSource {
    id: IngestionSourceId,
    rule_id: IngestionRuleId,
    fingerprint: SourceFingerprint,
    status: RuleStatus,
    job_id: Option<JobId>,
    created_at_ms: i64,
    updated_at_ms: i64,
}

impl IngestionSource {
    pub const fn id(&self) -> IngestionSourceId {
        self.id
    }

    pub const fn rule_id(&self) -> IngestionRuleId {
        self.rule_id
    }

    pub const fn fingerprint(&self) -> &SourceFingerprint {
        &self.fingerprint
    }

    pub const fn status(&self) -> RuleStatus {
        self.status
    }

    pub const fn job_id(&self) -> Option<JobId> {
        self.job_id
    }

    pub const fn created_at_ms(&self) -> i64 {
        self.created_at_ms
    }

    pub const fn updated_at_ms(&self) -> i64 {
        self.updated_at_ms
    }

    pub(crate) fn from_parts(parts: IngestionSourceParts) -> Self {
        Self {
            id: parts.id,
            rule_id: parts.rule_id,
            fingerprint: parts.fingerprint,
            status: parts.status,
            job_id: parts.job_id,
            created_at_ms: parts.created_at_ms,
            updated_at_ms: parts.updated_at_ms,
        }
    }
}

pub(crate) struct IngestionSourceParts {
    pub id: IngestionSourceId,
    pub rule_id: IngestionRuleId,
    pub fingerprint: SourceFingerprint,
    pub status: RuleStatus,
    pub job_id: Option<JobId>,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceReservation {
    Reserved(IngestionSource),
    Existing(IngestionSource),
}

impl SourceReservation {
    pub const fn is_reserved(&self) -> bool {
        matches!(self, Self::Reserved(_))
    }

    pub const fn source(&self) -> &IngestionSource {
        match self {
            Self::Reserved(source) | Self::Existing(source) => source,
        }
    }
}

#[derive(Debug, Clone, Copy, Error, PartialEq, Eq)]
pub enum IngestionModelError {
    #[error("{field} must contain between 1 and {max} bytes")]
    InvalidRequiredText { field: &'static str, max: usize },
    #[error("{field} must contain no more than {max} bytes")]
    InvalidOptionalText { field: &'static str, max: usize },
    #[error("ingestion source size exceeds SQLite integer range")]
    SourceSizeOutOfRange,
    #[error("ingestion source modified time must not be negative")]
    ModifiedTimeOutOfRange,
    #[error("ingestion rule id must be positive")]
    InvalidRuleId,
    #[error("ingestion source id must be positive")]
    InvalidSourceId,
    #[error("invalid persistent media kind mode")]
    InvalidMediaKindMode,
    #[error("invalid persistent media kind")]
    InvalidMediaKind,
    #[error("invalid persistent rule placement")]
    InvalidPlacement,
    #[error("invalid persistent rule status")]
    InvalidStatus,
}

fn validate_required(
    field: &'static str,
    value: &str,
    max: usize,
) -> Result<(), IngestionModelError> {
    if value.is_empty() || value.len() > max || value.chars().any(char::is_control) {
        return Err(IngestionModelError::InvalidRequiredText { field, max });
    }
    Ok(())
}

fn validate_optional(
    field: &'static str,
    value: &str,
    max: usize,
) -> Result<(), IngestionModelError> {
    if value.len() > max || value.chars().any(char::is_control) {
        return Err(IngestionModelError::InvalidOptionalText { field, max });
    }
    Ok(())
}

const fn media_kind_name(kind: JobMediaKind) -> &'static str {
    match kind {
        JobMediaKind::Anime => "anime",
        JobMediaKind::Book => "book",
        JobMediaKind::Movie => "movie",
        JobMediaKind::Music => "music",
        JobMediaKind::Television => "television",
    }
}

fn parse_media_kind(value: &str) -> Result<JobMediaKind, IngestionModelError> {
    match value {
        "anime" => Ok(JobMediaKind::Anime),
        "book" => Ok(JobMediaKind::Book),
        "movie" => Ok(JobMediaKind::Movie),
        "music" => Ok(JobMediaKind::Music),
        "television" => Ok(JobMediaKind::Television),
        _ => Err(IngestionModelError::InvalidMediaKind),
    }
}
