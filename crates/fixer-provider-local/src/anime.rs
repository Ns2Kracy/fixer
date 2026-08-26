//! Local anime NFO hierarchy parsing.

use crate::{LocalError, ScanWarning};
use fixer_core::{
    AnimeEpisode, AnimeEpisodeClass, AnimeSeries, AnimeSeriesRelation, Cour, LocalizedValue, WorkId,
};
use serde::Deserialize;
use std::{
    fs,
    path::{Path, PathBuf},
};

/// Anime documents and warnings produced by a local scan.
#[derive(Debug, Clone, Default)]
pub struct AnimeScanResult {
    pub documents: Vec<AnimeSeries>,
    /// Series roots aligned by index with `documents`.
    pub roots: Vec<PathBuf>,
    pub warnings: Vec<ScanWarning>,
}

#[derive(Debug, Deserialize)]
struct AnimeNfo {
    title: String,
    #[serde(default)]
    plot: Option<String>,
    relation: String,
}

#[derive(Debug, Deserialize)]
struct CourNfo {
    cour: u32,
}

#[derive(Debug, Deserialize)]
struct EpisodeNfo {
    title: String,
    cour: u32,
    #[serde(rename = "episodeclass")]
    episode_class: String,
    #[serde(default, rename = "airednumber")]
    aired_number: Option<u32>,
    #[serde(default, rename = "absolutenumber")]
    absolute_number: Option<u32>,
}

/// Recursively scans anime NFO hierarchies without following symbolic links.
pub fn scan_anime(root: &Path) -> Result<AnimeScanResult, LocalError> {
    if !root.is_dir() {
        return Err(LocalError::InvalidPath(root.to_path_buf()));
    }
    let mut metadata_paths = Vec::new();
    collect_anime_nfos(root, &mut metadata_paths)?;
    metadata_paths.sort();

    let mut result = AnimeScanResult::default();
    for metadata_path in metadata_paths {
        let series_root = metadata_path
            .parent()
            .ok_or_else(|| LocalError::InvalidPath(metadata_path.clone()))?;
        match parse_series(series_root, &metadata_path, &mut result.warnings) {
            Ok(series) => {
                result.documents.push(series);
                result.roots.push(series_root.to_path_buf());
            }
            Err(error) => result.warnings.push(ScanWarning {
                path: metadata_path,
                message: error.to_string(),
            }),
        }
    }
    Ok(result)
}

fn parse_series(
    series_root: &Path,
    metadata_path: &Path,
    warnings: &mut Vec<ScanWarning>,
) -> Result<AnimeSeries, LocalError> {
    let metadata: AnimeNfo = quick_xml::de::from_str(&fs::read_to_string(metadata_path)?)?;
    if metadata.title.trim().is_empty() {
        return Err(LocalError::InvalidMetadata(
            "anime title is required".to_owned(),
        ));
    }
    let relation = parse_relation(&metadata.relation)?;
    let series_slug = slug(&metadata.title);
    let mut cours = Vec::new();
    let mut directories = fs::read_dir(series_root)?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect::<Vec<_>>();
    directories.sort();
    for directory in directories {
        let cour_path = directory.join("cour.nfo");
        if !cour_path.is_file() {
            continue;
        }
        match parse_cour(&directory, &cour_path, &series_slug, warnings) {
            Ok(cour) => cours.push(cour),
            Err(error) => warnings.push(ScanWarning {
                path: cour_path,
                message: error.to_string(),
            }),
        }
    }
    cours.sort_by_key(|cour| cour.number);
    let mut titles = LocalizedValue::new();
    titles.insert("und", metadata.title)?;
    let mut series = AnimeSeries::new(
        WorkId::new(format!("local-anime-{series_slug}"))?,
        titles,
        relation,
        cours,
    );
    if let Some(plot) = metadata.plot.filter(|value| !value.trim().is_empty()) {
        series.summaries.insert("und", plot)?;
    }
    Ok(series)
}

fn parse_cour(
    directory: &Path,
    cour_path: &Path,
    series_slug: &str,
    warnings: &mut Vec<ScanWarning>,
) -> Result<Cour, LocalError> {
    let metadata: CourNfo = quick_xml::de::from_str(&fs::read_to_string(cour_path)?)?;
    if metadata.cour == 0 {
        return Err(LocalError::InvalidMetadata(
            "anime cour number must be positive".to_owned(),
        ));
    }
    let mut episode_paths = fs::read_dir(directory)?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_file()
                && path.file_name().and_then(|value| value.to_str()) != Some("cour.nfo")
                && path
                    .extension()
                    .and_then(|value| value.to_str())
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("nfo"))
        })
        .collect::<Vec<_>>();
    episode_paths.sort();
    let mut episodes = Vec::new();
    for path in episode_paths {
        match parse_episode(&path, metadata.cour, series_slug) {
            Ok(episode) => episodes.push(episode),
            Err(error) => warnings.push(ScanWarning {
                path,
                message: error.to_string(),
            }),
        }
    }
    episodes.sort_by_key(|episode| {
        (
            class_order(episode.class),
            episode.aired_number.unwrap_or(u32::MAX),
            episode.absolute_number.unwrap_or(u32::MAX),
        )
    });
    Ok(Cour::new(metadata.cour, episodes)?)
}

fn parse_episode(
    path: &Path,
    expected_cour: u32,
    series_slug: &str,
) -> Result<AnimeEpisode, LocalError> {
    let metadata: EpisodeNfo = quick_xml::de::from_str(&fs::read_to_string(path)?)?;
    if metadata.cour != expected_cour {
        return Err(LocalError::InvalidMetadata(format!(
            "episode cour {} does not match containing cour {expected_cour}",
            metadata.cour
        )));
    }
    if metadata.title.trim().is_empty() {
        return Err(LocalError::InvalidMetadata(
            "anime episode title is required".to_owned(),
        ));
    }
    let class = parse_class(&metadata.episode_class)?;
    let identity = path
        .file_stem()
        .and_then(|value| value.to_str())
        .map(slug)
        .ok_or_else(|| LocalError::InvalidPath(path.to_path_buf()))?;
    let mut titles = LocalizedValue::new();
    titles.insert("und", metadata.title)?;
    Ok(AnimeEpisode::new(
        WorkId::new(format!("local-anime-{series_slug}-{identity}"))?,
        titles,
        class,
        metadata.aired_number,
        metadata.absolute_number,
    )?)
}

fn collect_anime_nfos(directory: &Path, paths: &mut Vec<PathBuf>) -> Result<(), LocalError> {
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path)?;
        if metadata.file_type().is_symlink() {
            continue;
        }
        if metadata.is_dir() {
            collect_anime_nfos(&path, paths)?;
        } else if path.file_name().and_then(|value| value.to_str()) == Some("anime.nfo") {
            paths.push(path);
        }
    }
    Ok(())
}

fn parse_relation(value: &str) -> Result<AnimeSeriesRelation, LocalError> {
    match value.trim().to_ascii_lowercase().as_str() {
        "original" => Ok(AnimeSeriesRelation::Original),
        "adaptation" => Ok(AnimeSeriesRelation::Adaptation),
        "sequel" => Ok(AnimeSeriesRelation::Sequel),
        "prequel" => Ok(AnimeSeriesRelation::Prequel),
        "side_story" | "side story" => Ok(AnimeSeriesRelation::SideStory),
        "spin_off" | "spin-off" | "spin off" => Ok(AnimeSeriesRelation::SpinOff),
        other => Err(LocalError::InvalidMetadata(format!(
            "unsupported anime relation `{other}`"
        ))),
    }
}

fn parse_class(value: &str) -> Result<AnimeEpisodeClass, LocalError> {
    match value.trim().to_ascii_lowercase().as_str() {
        "regular" => Ok(AnimeEpisodeClass::Regular),
        "ova" => Ok(AnimeEpisodeClass::Ova),
        "ona" => Ok(AnimeEpisodeClass::Ona),
        "special" => Ok(AnimeEpisodeClass::Special),
        other => Err(LocalError::InvalidMetadata(format!(
            "unsupported anime episode class `{other}`"
        ))),
    }
}

const fn class_order(class: AnimeEpisodeClass) -> u8 {
    match class {
        AnimeEpisodeClass::Regular => 0,
        AnimeEpisodeClass::Ova => 1,
        AnimeEpisodeClass::Ona => 2,
        AnimeEpisodeClass::Special => 3,
    }
}

fn slug(value: &str) -> String {
    let slug = value
        .chars()
        .map(|character| {
            if character.is_alphanumeric() {
                character
            } else {
                '-'
            }
        })
        .collect::<String>()
        .split('-')
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>()
        .join("-")
        .to_lowercase();
    if slug.is_empty() {
        "untitled".to_owned()
    } else {
        slug
    }
}
