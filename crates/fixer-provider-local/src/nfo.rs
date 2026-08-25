//! Kodi/Jellyfin NFO movie subset.

use crate::LocalError;
use fixer_core::{LocalizedValue, Movie, MovieRelease, ReleaseDate, ReleaseId, WorkId};
use serde::Deserialize;

#[derive(Deserialize)]
struct MovieNfo {
    title: String,
    #[serde(default, rename = "originaltitle")]
    original_title: Option<String>,
    #[serde(default)]
    year: Option<u16>,
    #[serde(default)]
    plot: Option<String>,
}

/// Parses the supported local NFO movie subset.
pub fn parse_nfo(input: &str) -> Result<Movie, LocalError> {
    let dto: MovieNfo = quick_xml::de::from_str(input)?;
    if dto.title.trim().is_empty() {
        return Err(LocalError::InvalidMetadata(
            "NFO title is required".to_owned(),
        ));
    }
    let slug = dto
        .title
        .split_whitespace()
        .collect::<Vec<_>>()
        .join("-")
        .to_lowercase();
    let mut titles = LocalizedValue::new();
    titles.insert("und", dto.title)?;
    if let Some(original) = dto.original_title.filter(|value| !value.trim().is_empty()) {
        titles.insert("en", original)?;
    }
    let mut movie = Movie::new(WorkId::new(format!("nfo-{slug}"))?, titles);
    if let Some(plot) = dto.plot.filter(|value| !value.trim().is_empty()) {
        movie.summaries.insert("und", plot)?;
    }
    if let Some(year) = dto.year {
        movie.releases.push(MovieRelease::new(
            ReleaseId::new(format!("nfo-{slug}-{year}"))?,
            ReleaseDate::year(year)?,
        ));
    }
    Ok(movie)
}
