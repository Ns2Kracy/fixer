//! Deterministic candidate matching.

use crate::{Candidate, CoreError, ExternalId, LocalizedValue, MediaKind};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Typed matching input independent of provider implementation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MatchQuery {
    media_kind: MediaKind,
    title: String,
    localized_titles: LocalizedValue<String>,
    aliases: Vec<String>,
    external_ids: Vec<ExternalId>,
    year: Option<u16>,
    sequence: Option<String>,
}

impl MatchQuery {
    /// Constructs a movie matching query.
    pub fn movie(title: impl Into<String>) -> Result<Self, CoreError> {
        Self::new(MediaKind::Movie, title)
    }

    /// Constructs a television series matching query.
    pub fn television(title: impl Into<String>) -> Result<Self, CoreError> {
        Self::new(MediaKind::Television, title)
    }

    /// Constructs an anime matching query.
    pub fn anime(title: impl Into<String>) -> Result<Self, CoreError> {
        Self::new(MediaKind::Anime, title)
    }

    /// Constructs a music release-group matching query.
    pub fn music(title: impl Into<String>) -> Result<Self, CoreError> {
        Self::new(MediaKind::Music, title)
    }

    /// Constructs a book work matching query.
    pub fn book(title: impl Into<String>) -> Result<Self, CoreError> {
        Self::new(MediaKind::Book, title)
    }

    fn new(media_kind: MediaKind, title: impl Into<String>) -> Result<Self, CoreError> {
        let title = title.into();
        validate_text("match.title", &title)?;
        Ok(Self {
            media_kind,
            title,
            localized_titles: LocalizedValue::new(),
            aliases: Vec::new(),
            external_ids: Vec::new(),
            year: None,
            sequence: None,
        })
    }

    /// Adds a localized title alternative.
    pub fn add_localized_title(
        &mut self,
        language: impl AsRef<str>,
        title: impl Into<String>,
    ) -> Result<(), CoreError> {
        let title = title.into();
        validate_text("match.localized_title", &title)?;
        self.localized_titles.insert(language, title)
    }

    /// Adds an alias.
    pub fn with_alias(mut self, alias: impl Into<String>) -> Result<Self, CoreError> {
        let alias = alias.into();
        validate_text("match.alias", &alias)?;
        self.aliases.push(alias);
        Ok(self)
    }

    /// Adds an exact external ID.
    pub fn with_external_id(mut self, external_id: ExternalId) -> Self {
        if !self.external_ids.contains(&external_id) {
            self.external_ids.push(external_id);
        }
        self
    }

    /// Adds a release year.
    pub const fn with_year(mut self, year: u16) -> Self {
        self.year = Some(year);
        self
    }

    /// Adds a domain-specific sequence identifier.
    pub fn with_sequence(mut self, sequence: impl Into<String>) -> Result<Self, CoreError> {
        let sequence = sequence.into();
        validate_text("match.sequence", &sequence)?;
        self.sequence = Some(sequence);
        Ok(self)
    }
}

fn validate_text(field: &'static str, value: &str) -> Result<(), CoreError> {
    if value.trim().is_empty() || value.chars().any(char::is_control) {
        Err(CoreError::InvalidDomainValue {
            field,
            value: value.to_owned(),
        })
    } else {
        Ok(())
    }
}

/// A candidate in deterministic match order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RankedCandidate {
    pub candidate: Candidate,
}

/// Selection result retaining every ordered candidate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MatchSelection {
    ranked: Vec<RankedCandidate>,
}

impl MatchSelection {
    /// Returns candidates in deterministic match order.
    pub fn ranked(&self) -> &[RankedCandidate] {
        &self.ranked
    }

    /// Returns the first ordered candidate.
    pub fn selected(&self) -> Option<&RankedCandidate> {
        self.ranked.first()
    }
}

/// Matching failures.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum MatchingError {
    #[error("candidate media kind {candidate:?} does not match query {query:?}")]
    MediaKindMismatch {
        query: MediaKind,
        candidate: MediaKind,
    },
    #[error("no candidates were supplied")]
    NoCandidates,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum MatchClass {
    ProviderNative,
    ExactTitle,
    ExactTitleYear,
    ExactExternalId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum SequenceClass {
    Other,
    Exact,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct MatchKey {
    class: MatchClass,
    sequence: SequenceClass,
}

/// Deterministic matcher that keeps provider and provider-native order for ties.
#[derive(Debug, Clone, Copy, Default)]
pub struct Matcher;

impl Matcher {
    /// Orders candidates using exact identity and title categories.
    pub fn rank(
        &self,
        query: &MatchQuery,
        candidates: Vec<Candidate>,
    ) -> Result<Vec<RankedCandidate>, MatchingError> {
        let mut ranked = candidates
            .into_iter()
            .map(|candidate| match_key(query, &candidate).map(|key| (candidate, key)))
            .collect::<Result<Vec<_>, _>>()?;

        // `sort_by` is stable, so equal keys preserve configured provider and native result order.
        ranked.sort_by(|(_, left), (_, right)| right.cmp(left));

        Ok(ranked
            .into_iter()
            .map(|(candidate, _)| RankedCandidate { candidate })
            .collect())
    }

    /// Orders candidates and selects the first result.
    pub fn select(
        &self,
        query: &MatchQuery,
        candidates: Vec<Candidate>,
    ) -> Result<MatchSelection, MatchingError> {
        let ranked = self.rank(query, candidates)?;
        if ranked.is_empty() {
            return Err(MatchingError::NoCandidates);
        }
        Ok(MatchSelection { ranked })
    }
}

fn match_key(query: &MatchQuery, candidate: &Candidate) -> Result<MatchKey, MatchingError> {
    if query.media_kind != candidate.media_kind() {
        return Err(MatchingError::MediaKindMismatch {
            query: query.media_kind,
            candidate: candidate.media_kind(),
        });
    }

    let (title, year, sequence) = candidate_facts(candidate);
    let exact_external_id = query
        .external_ids
        .iter()
        .any(|id| id == candidate.external_id());
    let exact_title = exact_title(query, title);
    let class = if exact_external_id {
        MatchClass::ExactExternalId
    } else if exact_title && query.year.is_some() && query.year == year {
        MatchClass::ExactTitleYear
    } else if exact_title {
        MatchClass::ExactTitle
    } else {
        MatchClass::ProviderNative
    };
    let sequence = match (&query.sequence, sequence) {
        (Some(expected), Some(actual)) if normalize(expected) == normalize(actual) => {
            SequenceClass::Exact
        }
        _ => SequenceClass::Other,
    };

    Ok(MatchKey { class, sequence })
}

fn exact_title(query: &MatchQuery, candidate_title: &str) -> bool {
    let candidate = normalize(candidate_title);
    normalize(&query.title) == candidate
        || query
            .localized_titles
            .entries()
            .iter()
            .any(|entry| normalize(entry.value()) == candidate)
        || query
            .aliases
            .iter()
            .any(|alias| normalize(alias) == candidate)
}

fn candidate_facts(candidate: &Candidate) -> (&str, Option<u16>, Option<&str>) {
    match candidate {
        Candidate::Movie(value) => (&value.title, value.year, value.sequence.as_deref()),
        Candidate::Television(value) => (&value.title, value.year, value.sequence.as_deref()),
        Candidate::Anime(value) => (&value.title, value.year, value.sequence.as_deref()),
        Candidate::Music(value) => (&value.title, value.year, value.sequence.as_deref()),
        Candidate::Book(value) => (&value.title, value.year, value.sequence.as_deref()),
    }
}

fn normalize(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}
