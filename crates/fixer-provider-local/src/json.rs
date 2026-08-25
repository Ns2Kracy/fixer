//! Sanitized local JSON movie parsing.

use crate::LocalError;
use fixer_core::{ExternalId, LocalizedValue, Movie, MovieRelease, ReleaseDate, ReleaseId, WorkId};
use serde::Deserialize;
use std::collections::BTreeMap;

#[derive(Deserialize)]
struct MovieJson {
    id: ExternalId,
    titles: BTreeMap<String, String>,
    year: Option<u16>,
    #[serde(default)]
    summary: BTreeMap<String, String>,
}

/// Parses the supported local JSON movie subset.
pub fn parse_json(input: &str) -> Result<Movie, LocalError> {
    let dto: MovieJson = serde_json::from_str(input)?;
    let mut titles = LocalizedValue::new();
    for (language, value) in dto.titles {
        titles.insert(language, value)?;
    }
    if titles.entries().is_empty() {
        return Err(LocalError::InvalidMetadata(
            "movie titles are required".to_owned(),
        ));
    }
    let mut movie = Movie::new(
        WorkId::new(format!("{}-{}", dto.id.namespace, dto.id.value))?,
        titles,
    );
    for (language, value) in dto.summary {
        movie.summaries.insert(language, value)?;
    }
    if let Some(year) = dto.year {
        movie.releases.push(MovieRelease::new(
            ReleaseId::new(format!("{}-{}-release", dto.id.namespace, dto.id.value))?,
            ReleaseDate::year(year)?,
        ));
    }
    Ok(movie)
}
