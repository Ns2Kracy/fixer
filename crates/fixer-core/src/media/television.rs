//! Television series, seasons, episodes, and ordering.

use super::common::{ArtworkReference, Credit, Duration, Summaries, Titles, WorkId};
use crate::CoreError;
use serde::{Deserialize, Serialize};

/// Numbering scheme used to interpret episodes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrderingScheme {
    Aired,
    Dvd,
    Absolute,
}

/// Typed television episode sequence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct EpisodeSequence {
    pub scheme: OrderingScheme,
    pub season: Option<u32>,
    pub episode: u32,
}

impl EpisodeSequence {
    /// Constructs a positive aired season/episode sequence.
    pub fn aired(season: u32, episode: u32) -> Result<Self, CoreError> {
        if episode == 0 {
            return Err(CoreError::InvalidDomainValue {
                field: "episode_sequence",
                value: format!("S{season}E{episode}"),
            });
        }
        Ok(Self {
            scheme: OrderingScheme::Aired,
            season: Some(season),
            episode,
        })
    }

    /// Constructs a positive absolute episode sequence.
    pub fn absolute(episode: u32) -> Result<Self, CoreError> {
        if episode == 0 {
            return Err(CoreError::InvalidDomainValue {
                field: "episode_sequence",
                value: episode.to_string(),
            });
        }
        Ok(Self {
            scheme: OrderingScheme::Absolute,
            season: None,
            episode,
        })
    }
}

/// One television episode.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Episode {
    pub id: WorkId,
    pub titles: Titles,
    pub summaries: Summaries,
    pub sequence: EpisodeSequence,
    pub runtime: Option<Duration>,
    pub credits: Vec<Credit>,
    pub artwork: Vec<ArtworkReference>,
}

impl Episode {
    /// Constructs an episode.
    pub fn new(id: WorkId, titles: Titles, sequence: EpisodeSequence) -> Self {
        Self {
            id,
            titles,
            summaries: Summaries::new(),
            sequence,
            runtime: None,
            credits: Vec::new(),
            artwork: Vec::new(),
        }
    }
}

/// A numbered television season.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Season {
    pub id: WorkId,
    pub number: u32,
    pub episodes: Vec<Episode>,
    pub artwork: Vec<ArtworkReference>,
}

impl Season {
    /// Constructs a season. Zero represents specials.
    pub fn new(id: WorkId, number: u32, episodes: Vec<Episode>) -> Result<Self, CoreError> {
        Ok(Self {
            id,
            number,
            episodes,
            artwork: Vec::new(),
        })
    }
}

/// A television series hierarchy.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Series {
    pub id: WorkId,
    pub titles: Titles,
    pub summaries: Summaries,
    pub ordering: OrderingScheme,
    pub seasons: Vec<Season>,
    pub artwork: Vec<ArtworkReference>,
}

impl Series {
    /// Constructs a series.
    pub fn new(id: WorkId, titles: Titles, ordering: OrderingScheme, seasons: Vec<Season>) -> Self {
        Self {
            id,
            titles,
            summaries: Summaries::new(),
            ordering,
            seasons,
            artwork: Vec::new(),
        }
    }
}
