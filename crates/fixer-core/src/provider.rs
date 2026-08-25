//! Object-safe metadata provider contracts.

use crate::{
    AnimeSeries, BookWork, CoreError, ExternalId, LanguageTag, Movie, MusicReleaseGroup,
    ProviderId, Series,
};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, future::Future, pin::Pin};
use thiserror::Error;

use crate::HttpClient;

/// A boxed future that does not require a particular async runtime.
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Supported top-level media domains.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaKind {
    Movie,
    Television,
    Anime,
    Music,
    Book,
}

/// Stable provider identity and declared capabilities.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderDescriptor {
    id: ProviderId,
    display_name: String,
    media_kinds: BTreeSet<MediaKind>,
    requires_network: bool,
}

impl ProviderDescriptor {
    /// Constructs a provider descriptor with at least one capability.
    pub fn new(
        id: ProviderId,
        display_name: impl Into<String>,
        media_kinds: impl IntoIterator<Item = MediaKind>,
    ) -> Result<Self, CoreError> {
        let display_name = display_name.into();
        if display_name.trim().is_empty() {
            return Err(CoreError::InvalidDomainValue {
                field: "provider.display_name",
                value: display_name,
            });
        }
        let media_kinds = media_kinds.into_iter().collect::<BTreeSet<_>>();
        if media_kinds.is_empty() {
            return Err(CoreError::InvalidDomainValue {
                field: "provider.media_kinds",
                value: "empty".to_owned(),
            });
        }
        Ok(Self {
            id,
            display_name,
            media_kinds,
            requires_network: true,
        })
    }

    /// Returns the stable provider ID.
    pub fn id(&self) -> &ProviderId {
        &self.id
    }
    /// Returns the human-readable provider name.
    pub fn display_name(&self) -> &str {
        &self.display_name
    }
    /// Returns whether this provider uses network access.
    pub const fn requires_network(&self) -> bool {
        self.requires_network
    }
    /// Marks whether this provider requires network access.
    pub const fn with_network_requirement(mut self, required: bool) -> Self {
        self.requires_network = required;
        self
    }
    /// Reports support for a media domain.
    pub fn supports(&self, media_kind: MediaKind) -> bool {
        self.media_kinds.contains(&media_kind)
    }
    /// Returns a structured error when a capability is absent.
    pub fn ensure_support(&self, media_kind: MediaKind) -> Result<(), ProviderError> {
        if self.supports(media_kind) {
            Ok(())
        } else {
            Err(ProviderError::UnsupportedMedia {
                provider: self.id.clone(),
                media_kind,
            })
        }
    }
}

/// Typed provider search input.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "media_kind", rename_all = "snake_case")]
pub enum SearchRequest {
    Movie {
        title: String,
        year: Option<u16>,
        locales: Vec<LanguageTag>,
    },
    Television {
        title: String,
        year: Option<u16>,
        locales: Vec<LanguageTag>,
    },
    Anime {
        title: String,
        year: Option<u16>,
        locales: Vec<LanguageTag>,
    },
    Music {
        title: String,
        year: Option<u16>,
        locales: Vec<LanguageTag>,
    },
    Book {
        title: String,
        year: Option<u16>,
        locales: Vec<LanguageTag>,
    },
}

impl SearchRequest {
    /// Constructs a movie title search.
    pub fn movie(title: impl Into<String>, year: Option<u16>) -> Result<Self, CoreError> {
        let title = title.into();
        validate_query_title(&title)?;
        Ok(Self::Movie {
            title,
            year,
            locales: Vec::new(),
        })
    }

    /// Returns the requested media domain.
    pub const fn media_kind(&self) -> MediaKind {
        match self {
            Self::Movie { .. } => MediaKind::Movie,
            Self::Television { .. } => MediaKind::Television,
            Self::Anime { .. } => MediaKind::Anime,
            Self::Music { .. } => MediaKind::Music,
            Self::Book { .. } => MediaKind::Book,
        }
    }

    /// Returns the title query when this is a title-based request.
    pub fn title(&self) -> Option<&str> {
        Some(match self {
            Self::Movie { title, .. }
            | Self::Television { title, .. }
            | Self::Anime { title, .. }
            | Self::Music { title, .. }
            | Self::Book { title, .. } => title,
        })
    }

    /// Returns the optional release year constraint.
    pub const fn year(&self) -> Option<u16> {
        match self {
            Self::Movie { year, .. }
            | Self::Television { year, .. }
            | Self::Anime { year, .. }
            | Self::Music { year, .. }
            | Self::Book { year, .. } => *year,
        }
    }

    /// Replaces requested locales.
    pub fn with_locales(mut self, locales: Vec<LanguageTag>) -> Self {
        match &mut self {
            Self::Movie { locales: value, .. }
            | Self::Television { locales: value, .. }
            | Self::Anime { locales: value, .. }
            | Self::Music { locales: value, .. }
            | Self::Book { locales: value, .. } => *value = locales,
        }
        self
    }
}

fn validate_query_title(title: &str) -> Result<(), CoreError> {
    if title.trim().is_empty() || title.chars().any(char::is_control) {
        Err(CoreError::InvalidDomainValue {
            field: "search.title",
            value: title.to_owned(),
        })
    } else {
        Ok(())
    }
}

macro_rules! candidate {
    ($name:ident) => {
        /// A typed search result for this media domain.
        #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
        pub struct $name {
            pub provider: ProviderId,
            pub external_id: ExternalId,
            pub title: String,
            pub year: Option<u16>,
            pub sequence: Option<String>,
        }
        impl $name {
            /// Constructs a candidate.
            pub fn new(
                provider: ProviderId,
                external_id: ExternalId,
                title: impl Into<String>,
                year: Option<u16>,
            ) -> Result<Self, CoreError> {
                let title = title.into();
                validate_query_title(&title)?;
                Ok(Self {
                    provider,
                    external_id,
                    title,
                    year,
                    sequence: None,
                })
            }

            /// Adds a domain-specific sequence identifier.
            pub fn with_sequence(mut self, sequence: impl Into<String>) -> Result<Self, CoreError> {
                let sequence = sequence.into();
                validate_query_title(&sequence)?;
                self.sequence = Some(sequence);
                Ok(self)
            }
        }
    };
}

candidate!(MovieCandidate);
candidate!(TelevisionCandidate);
candidate!(AnimeCandidate);
candidate!(MusicCandidate);
candidate!(BookCandidate);

/// A heterogeneous typed provider search result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "media_kind", content = "candidate", rename_all = "snake_case")]
pub enum Candidate {
    Movie(MovieCandidate),
    Television(TelevisionCandidate),
    Anime(AnimeCandidate),
    Music(MusicCandidate),
    Book(BookCandidate),
}

impl Candidate {
    /// Returns the candidate's media domain.
    pub const fn media_kind(&self) -> MediaKind {
        match self {
            Self::Movie(_) => MediaKind::Movie,
            Self::Television(_) => MediaKind::Television,
            Self::Anime(_) => MediaKind::Anime,
            Self::Music(_) => MediaKind::Music,
            Self::Book(_) => MediaKind::Book,
        }
    }
    /// Returns the provider ID.
    pub fn provider(&self) -> &ProviderId {
        match self {
            Self::Movie(v) => &v.provider,
            Self::Television(v) => &v.provider,
            Self::Anime(v) => &v.provider,
            Self::Music(v) => &v.provider,
            Self::Book(v) => &v.provider,
        }
    }
    /// Returns the external ID selected for fetching.
    pub fn external_id(&self) -> &ExternalId {
        match self {
            Self::Movie(v) => &v.external_id,
            Self::Television(v) => &v.external_id,
            Self::Anime(v) => &v.external_id,
            Self::Music(v) => &v.external_id,
            Self::Book(v) => &v.external_id,
        }
    }
}

/// Typed provider fetch input.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FetchRequest {
    pub media_kind: MediaKind,
    pub external_id: ExternalId,
    pub locales: Vec<LanguageTag>,
}

impl FetchRequest {
    /// Constructs a fetch request.
    pub const fn new(media_kind: MediaKind, external_id: ExternalId) -> Self {
        Self {
            media_kind,
            external_id,
            locales: Vec::new(),
        }
    }
    /// Returns the fetched media domain.
    pub const fn media_kind(&self) -> MediaKind {
        self.media_kind
    }
    /// Replaces requested locales.
    pub fn with_locales(mut self, locales: Vec<LanguageTag>) -> Self {
        self.locales = locales;
        self
    }
}

/// A heterogeneous typed metadata document.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "media_kind", content = "document", rename_all = "snake_case")]
pub enum MetadataDocument {
    Movie(Movie),
    Television(Series),
    Anime(AnimeSeries),
    Music(MusicReleaseGroup),
    Book(BookWork),
}

impl MetadataDocument {
    /// Returns the document's media domain.
    pub const fn media_kind(&self) -> MediaKind {
        match self {
            Self::Movie(_) => MediaKind::Movie,
            Self::Television(_) => MediaKind::Television,
            Self::Anime(_) => MediaKind::Anime,
            Self::Music(_) => MediaKind::Music,
            Self::Book(_) => MediaKind::Book,
        }
    }
}

/// Structured provider failures.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[non_exhaustive]
pub enum ProviderError {
    #[error("provider `{provider}` does not support {media_kind:?}")]
    UnsupportedMedia {
        provider: ProviderId,
        media_kind: MediaKind,
    },
    #[error("provider rejected input: {0}")]
    InvalidInput(String),
    #[error("provider transport failed: {0}")]
    Transport(String),
    #[error("provider response was invalid: {0}")]
    InvalidResponse(String),
    #[error("provider did not find the requested item")]
    NotFound,
}

impl From<CoreError> for ProviderError {
    fn from(error: CoreError) -> Self {
        Self::InvalidInput(error.to_string())
    }
}

/// Runtime-neutral metadata provider contract.
pub trait Provider: Send + Sync {
    fn descriptor(&self) -> &ProviderDescriptor;
    fn search<'a>(
        &'a self,
        request: SearchRequest,
        http: &'a dyn HttpClient,
    ) -> BoxFuture<'a, Result<Vec<Candidate>, ProviderError>>;
    fn fetch<'a>(
        &'a self,
        request: FetchRequest,
        http: &'a dyn HttpClient,
    ) -> BoxFuture<'a, Result<MetadataDocument, ProviderError>>;
}
