//! Artists, release groups, releases, discs, and tracks.

use super::common::{AssetId, Duration, ReleaseId, Titles, WorkId, validate_text};
use crate::CoreError;
use serde::{Deserialize, Serialize};

/// A music artist.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MusicArtist {
    pub id: WorkId,
    pub name: String,
}

impl MusicArtist {
    /// Constructs an artist.
    pub fn new(id: WorkId, name: impl Into<String>) -> Result<Self, CoreError> {
        let name = name.into();
        validate_text("artist.name", &name, 512)?;
        Ok(Self { id, name })
    }
}

/// Typed disc and track position.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrackSequence {
    pub disc: u32,
    pub track: u32,
}

impl TrackSequence {
    /// Constructs a positive track position.
    pub fn new(disc: u32, track: u32) -> Result<Self, CoreError> {
        if disc == 0 || track == 0 {
            return Err(CoreError::InvalidDomainValue {
                field: "track_sequence",
                value: format!("{disc}-{track}"),
            });
        }
        Ok(Self { disc, track })
    }
}

/// One audio track.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Track {
    pub id: AssetId,
    pub titles: Titles,
    pub sequence: TrackSequence,
    pub duration: Duration,
}

impl Track {
    /// Constructs a track.
    pub const fn new(
        id: AssetId,
        titles: Titles,
        sequence: TrackSequence,
        duration: Duration,
    ) -> Self {
        Self {
            id,
            titles,
            sequence,
            duration,
        }
    }
}

/// A numbered release disc.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Disc {
    pub number: u32,
    pub tracks: Vec<Track>,
}

impl Disc {
    /// Constructs a positive-numbered disc.
    pub fn new(number: u32, tracks: Vec<Track>) -> Result<Self, CoreError> {
        if number == 0 {
            return Err(CoreError::InvalidDomainValue {
                field: "disc.number",
                value: number.to_string(),
            });
        }
        Ok(Self { number, tracks })
    }
}

/// A particular album release.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MusicRelease {
    pub id: ReleaseId,
    pub discs: Vec<Disc>,
}
impl MusicRelease {
    pub const fn new(id: ReleaseId, discs: Vec<Disc>) -> Self {
        Self { id, discs }
    }
}

/// An abstract album or release group.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MusicReleaseGroup {
    pub id: WorkId,
    pub titles: Titles,
    pub artist: MusicArtist,
    pub releases: Vec<MusicRelease>,
}
impl MusicReleaseGroup {
    /// Constructs a release group.
    pub const fn new(
        id: WorkId,
        titles: Titles,
        artist: MusicArtist,
        releases: Vec<MusicRelease>,
    ) -> Self {
        Self {
            id,
            titles,
            artist,
            releases,
        }
    }
}
