//! Anime-specific hierarchy and episode numbering.

use super::common::{Summaries, Titles, WorkId};
use crate::CoreError;
use serde::{Deserialize, Serialize};

/// Relation between the anime and another abstract work.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnimeSeriesRelation {
    Original,
    Adaptation,
    Sequel,
    Prequel,
    SideStory,
    SpinOff,
}

/// Anime episode classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnimeEpisodeClass {
    Regular,
    Ova,
    Special,
}

/// An anime episode with aired and absolute numbering when known.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnimeEpisode {
    pub id: WorkId,
    pub titles: Titles,
    pub class: AnimeEpisodeClass,
    pub aired_number: Option<u32>,
    pub absolute_number: Option<u32>,
}

impl AnimeEpisode {
    /// Constructs a classified anime episode.
    pub fn new(
        id: WorkId,
        titles: Titles,
        class: AnimeEpisodeClass,
        aired_number: Option<u32>,
        absolute_number: Option<u32>,
    ) -> Result<Self, CoreError> {
        if aired_number == Some(0)
            || absolute_number == Some(0)
            || (aired_number.is_none() && absolute_number.is_none())
        {
            return Err(CoreError::InvalidDomainValue {
                field: "anime_episode.number",
                value: format!("aired={aired_number:?}, absolute={absolute_number:?}"),
            });
        }
        Ok(Self {
            id,
            titles,
            class,
            aired_number,
            absolute_number,
        })
    }
}

/// One production cour or season.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cour {
    pub number: u32,
    pub episodes: Vec<AnimeEpisode>,
}

impl Cour {
    /// Constructs a positive-numbered cour.
    pub fn new(number: u32, episodes: Vec<AnimeEpisode>) -> Result<Self, CoreError> {
        if number == 0 {
            return Err(CoreError::InvalidDomainValue {
                field: "cour.number",
                value: number.to_string(),
            });
        }
        Ok(Self { number, episodes })
    }
}

/// An anime work and its production cours.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnimeSeries {
    pub id: WorkId,
    pub titles: Titles,
    pub summaries: Summaries,
    pub relation: AnimeSeriesRelation,
    pub cours: Vec<Cour>,
}

impl AnimeSeries {
    /// Constructs an anime series.
    pub fn new(
        id: WorkId,
        titles: Titles,
        relation: AnimeSeriesRelation,
        cours: Vec<Cour>,
    ) -> Self {
        Self {
            id,
            titles,
            summaries: Summaries::new(),
            relation,
            cours,
        }
    }
}
