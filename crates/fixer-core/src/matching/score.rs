//! Explainable matching scores and evidence.

use crate::{Candidate, CoreError, ExternalId, LocalizedValue, MediaKind};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Evidence categories emitted by the deterministic matcher.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MatchEvidenceKind {
    ExternalId,
    Title,
    Alias,
    Year,
    Sequence,
}

/// One positive or negative contribution to a match score.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MatchEvidence {
    pub kind: MatchEvidenceKind,
    pub points: i32,
    pub detail: String,
}

/// Total score and its explainable components.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MatchScore {
    pub total: i32,
    pub evidence: Vec<MatchEvidence>,
}

impl MatchScore {
    fn new(evidence: Vec<MatchEvidence>) -> Self {
        let total = evidence.iter().map(|item| item.points).sum();
        Self { total, evidence }
    }
}

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
        let title = title.into();
        validate_text("match.title", &title)?;
        Ok(Self {
            media_kind: MediaKind::Movie,
            title,
            localized_titles: LocalizedValue::new(),
            aliases: Vec::new(),
            external_ids: Vec::new(),
            year: None,
            sequence: None,
        })
    }
    /// Constructs a television series matching query.
    pub fn television(title: impl Into<String>) -> Result<Self, CoreError> {
        let title = title.into();
        validate_text("match.title", &title)?;
        Ok(Self {
            media_kind: MediaKind::Television,
            title,
            localized_titles: LocalizedValue::new(),
            aliases: Vec::new(),
            external_ids: Vec::new(),
            year: None,
            sequence: None,
        })
    }
    /// Constructs an anime matching query.
    pub fn anime(title: impl Into<String>) -> Result<Self, CoreError> {
        let title = title.into();
        validate_text("match.title", &title)?;
        Ok(Self {
            media_kind: MediaKind::Anime,
            title,
            localized_titles: LocalizedValue::new(),
            aliases: Vec::new(),
            external_ids: Vec::new(),
            year: None,
            sequence: None,
        })
    }
    /// Constructs a music release-group matching query.
    pub fn music(title: impl Into<String>) -> Result<Self, CoreError> {
        let title = title.into();
        validate_text("match.title", &title)?;
        Ok(Self {
            media_kind: MediaKind::Music,
            title,
            localized_titles: LocalizedValue::new(),
            aliases: Vec::new(),
            external_ids: Vec::new(),
            year: None,
            sequence: None,
        })
    }
    /// Constructs a book work matching query.
    pub fn book(title: impl Into<String>) -> Result<Self, CoreError> {
        let title = title.into();
        validate_text("match.title", &title)?;
        Ok(Self {
            media_kind: MediaKind::Book,
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
        self.external_ids.push(external_id);
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

/// A candidate paired with its score.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RankedCandidate {
    pub candidate: Candidate,
    pub score: MatchScore,
}

/// Selection result retaining all ranked candidates and ambiguity state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MatchSelection {
    ranked: Vec<RankedCandidate>,
    ambiguous: bool,
}
impl MatchSelection {
    /// Returns candidates from highest to lowest score.
    pub fn ranked(&self) -> &[RankedCandidate] {
        &self.ranked
    }
    /// Reports whether the top two candidates tied.
    pub const fn is_ambiguous(&self) -> bool {
        self.ambiguous
    }
    /// Returns the unique top candidate when unambiguous.
    pub fn selected(&self) -> Option<&RankedCandidate> {
        if self.ambiguous {
            None
        } else {
            self.ranked.first()
        }
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

/// Deterministic baseline matcher with fixed, visible evidence weights.
#[derive(Debug, Clone, Copy, Default)]
pub struct Matcher;

impl Matcher {
    /// Scores one candidate.
    pub fn score(
        &self,
        query: &MatchQuery,
        candidate: &Candidate,
    ) -> Result<MatchScore, MatchingError> {
        if query.media_kind != candidate.media_kind() {
            return Err(MatchingError::MediaKindMismatch {
                query: query.media_kind,
                candidate: candidate.media_kind(),
            });
        }
        let (title, year, sequence) = candidate_facts(candidate);
        let mut evidence = Vec::new();

        if query
            .external_ids
            .iter()
            .any(|id| id == candidate.external_id())
        {
            evidence.push(MatchEvidence {
                kind: MatchEvidenceKind::ExternalId,
                points: 10_000,
                detail: format!(
                    "exact {}:{}",
                    candidate.external_id().namespace,
                    candidate.external_id().value
                ),
            });
        }

        let normalized_candidate = normalize(title);
        let localized_exact = query
            .localized_titles
            .entries()
            .iter()
            .any(|entry| normalize(entry.value()) == normalized_candidate);
        if normalize(&query.title) == normalized_candidate || localized_exact {
            evidence.push(MatchEvidence {
                kind: MatchEvidenceKind::Title,
                points: 100,
                detail: "exact normalized title".to_owned(),
            });
        } else {
            let similarity = token_similarity(&query.title, title);
            if similarity > 0 {
                evidence.push(MatchEvidence {
                    kind: MatchEvidenceKind::Title,
                    points: similarity * 40 / 100,
                    detail: format!("token overlap {similarity}%"),
                });
            }
        }

        if !query.aliases.is_empty() {
            let matched = query
                .aliases
                .iter()
                .any(|alias| normalize(alias) == normalized_candidate);
            evidence.push(MatchEvidence {
                kind: MatchEvidenceKind::Alias,
                points: if matched { 70 } else { -10 },
                detail: if matched {
                    "candidate title matched query alias".to_owned()
                } else {
                    "candidate title did not match any query alias".to_owned()
                },
            });
        }
        if let (Some(expected), Some(actual)) = (query.year, year) {
            evidence.push(MatchEvidence {
                kind: MatchEvidenceKind::Year,
                points: if expected == actual { 20 } else { -30 },
                detail: format!("expected {expected}, candidate {actual}"),
            });
        }
        if let (Some(expected), Some(actual)) = (&query.sequence, sequence) {
            evidence.push(MatchEvidence {
                kind: MatchEvidenceKind::Sequence,
                points: if normalize(expected) == normalize(actual) {
                    50
                } else {
                    -40
                },
                detail: format!("expected {expected}, candidate {actual}"),
            });
        }
        Ok(MatchScore::new(evidence))
    }

    /// Scores and deterministically ranks candidates.
    pub fn rank(
        &self,
        query: &MatchQuery,
        candidates: Vec<Candidate>,
    ) -> Result<Vec<RankedCandidate>, MatchingError> {
        let mut ranked = candidates
            .into_iter()
            .map(|candidate| {
                self.score(query, &candidate)
                    .map(|score| RankedCandidate { candidate, score })
            })
            .collect::<Result<Vec<_>, _>>()?;
        ranked.sort_by(|left, right| {
            right
                .score
                .total
                .cmp(&left.score.total)
                .then_with(|| left.candidate.provider().cmp(right.candidate.provider()))
                .then_with(|| {
                    left.candidate
                        .external_id()
                        .cmp(right.candidate.external_id())
                })
        });
        Ok(ranked)
    }

    /// Ranks candidates and marks equal top scores as ambiguous.
    pub fn select(
        &self,
        query: &MatchQuery,
        candidates: Vec<Candidate>,
    ) -> Result<MatchSelection, MatchingError> {
        let ranked = self.rank(query, candidates)?;
        if ranked.is_empty() {
            return Err(MatchingError::NoCandidates);
        }
        let ambiguous = ranked
            .get(1)
            .is_some_and(|second| second.score.total == ranked[0].score.total);
        Ok(MatchSelection { ranked, ambiguous })
    }
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
fn token_similarity(left: &str, right: &str) -> i32 {
    let left = normalize(left);
    let right = normalize(right);
    let left = left
        .split(' ')
        .filter(|v| !v.is_empty())
        .collect::<std::collections::BTreeSet<_>>();
    let right = right
        .split(' ')
        .filter(|v| !v.is_empty())
        .collect::<std::collections::BTreeSet<_>>();
    let union = left.union(&right).count();
    left.intersection(&right)
        .count()
        .saturating_mul(100)
        .checked_div(union)
        .and_then(|value| i32::try_from(value).ok())
        .unwrap_or(0)
}
