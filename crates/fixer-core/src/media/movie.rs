//! Movie works and their releases.

use super::common::{
    ArtworkReference, ContentRating, Credit, Duration, Genre, Rating, ReleaseDate, ReleaseId,
    Summaries, Titles, WorkId,
};
use serde::{Deserialize, Serialize};

/// A particular movie release or edition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MovieRelease {
    /// Stable release identity.
    pub id: ReleaseId,
    /// Release date.
    pub release_date: ReleaseDate,
    /// Edition label, when supplied.
    pub edition: Option<String>,
    /// Runtime for this cut, when known.
    pub runtime: Option<Duration>,
}

impl MovieRelease {
    /// Constructs a dated movie release.
    pub const fn new(id: ReleaseId, release_date: ReleaseDate) -> Self {
        Self {
            id,
            release_date,
            edition: None,
            runtime: None,
        }
    }
}

/// An abstract movie work and its known releases.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Movie {
    pub id: WorkId,
    pub titles: Titles,
    pub summaries: Summaries,
    pub releases: Vec<MovieRelease>,
    pub credits: Vec<Credit>,
    pub genres: Vec<Genre>,
    pub artwork: Vec<ArtworkReference>,
    pub ratings: Vec<Rating>,
    pub content_ratings: Vec<ContentRating>,
}

impl Movie {
    /// Constructs a movie with empty optional metadata collections.
    pub fn new(id: WorkId, titles: Titles) -> Self {
        Self {
            id,
            titles,
            summaries: Summaries::new(),
            releases: Vec::new(),
            credits: Vec::new(),
            genres: Vec::new(),
            artwork: Vec::new(),
            ratings: Vec::new(),
            content_ratings: Vec::new(),
        }
    }

    /// Returns the earliest known release year.
    pub fn release_year(&self) -> Option<u16> {
        self.releases
            .iter()
            .map(|release| release.release_date.year)
            .min()
    }
}
