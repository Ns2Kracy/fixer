//! Local television episode identification and hierarchy aggregation.

use crate::{LocalError, ScanWarning};
use fixer_core::{
    Episode, EpisodeSequence, ExternalId, LocalizedValue, OrderingScheme, Season, Series, WorkId,
};
use serde::Deserialize;
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

/// Evidence-bearing television episode facts inferred from a local path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EpisodeHint {
    pub series_title: String,
    pub episode_title: Option<String>,
    pub sequence: EpisodeSequence,
    pub external_ids: Vec<ExternalId>,
}

/// Supported Matroska television tags.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MatroskaEpisodeTags {
    pub series_title: Option<String>,
    pub episode_title: Option<String>,
    pub ordering: Option<OrderingScheme>,
    pub season: Option<u32>,
    pub episode: Option<u32>,
    pub external_ids: Vec<ExternalId>,
}

/// Television documents and warnings produced by a local scan.
#[derive(Debug, Clone, Default)]
pub struct TelevisionScanResult {
    pub documents: Vec<Series>,
    /// Series roots aligned by index with `documents`.
    pub roots: Vec<PathBuf>,
    pub warnings: Vec<ScanWarning>,
}

#[derive(Debug, Clone)]
pub struct TelevisionRecord {
    pub external_id: ExternalId,
    pub value: Series,
    pub root: PathBuf,
}

#[derive(Debug, Deserialize)]
struct TagsXml {
    #[serde(rename = "Tag", default)]
    tags: Vec<TagXml>,
}

#[derive(Debug, Deserialize)]
struct TagXml {
    #[serde(rename = "Simple", default)]
    simple: Vec<SimpleXml>,
}

#[derive(Debug, Deserialize)]
struct SimpleXml {
    #[serde(rename = "Name")]
    name: String,
    #[serde(rename = "String")]
    value: String,
}

/// Identifies one television episode from `S01E02` or a season-folder layout.
pub fn identify_episode_path(path: &Path) -> Result<EpisodeHint, LocalError> {
    let stem = path
        .file_stem()
        .and_then(|value| value.to_str())
        .ok_or_else(|| LocalError::InvalidPath(path.to_path_buf()))?;
    let parent = path
        .parent()
        .and_then(Path::file_name)
        .and_then(|v| v.to_str());
    let (folder_season, season_folder) = parent
        .and_then(parse_season_folder)
        .map_or((None, false), |season| (Some(season), true));

    if let Some((start, end, season, episode)) = parse_sxe(stem) {
        let raw_title = clean_name(&stem[..start]);
        let series_title = if raw_title.is_empty() {
            series_folder_title(path, season_folder)?
        } else {
            raw_title
        };
        let suffix = clean_episode_suffix(&stem[end..]);
        return Ok(EpisodeHint {
            series_title,
            episode_title: (!suffix.is_empty()).then_some(suffix),
            sequence: EpisodeSequence::aired(season, episode)?,
            external_ids: parse_external_ids(path),
        });
    }

    if let Some(season) = folder_season {
        if let Some((episode, remainder)) = parse_episode_prefix(stem) {
            return Ok(EpisodeHint {
                series_title: series_folder_title(path, true)?,
                episode_title: (!remainder.is_empty()).then_some(remainder),
                sequence: EpisodeSequence::aired(season, episode)?,
                external_ids: parse_external_ids(path),
            });
        }
    }

    Err(LocalError::Unidentified(path.to_path_buf()))
}

/// Returns the series root containing an episode path.
pub fn episode_series_root(path: &Path) -> Result<PathBuf, LocalError> {
    let directory = path
        .parent()
        .ok_or_else(|| LocalError::InvalidPath(path.to_path_buf()))?;
    let has_season_folder = directory
        .file_name()
        .and_then(|value| value.to_str())
        .and_then(parse_season_folder)
        .is_some();
    Ok(if has_season_folder {
        directory.parent().unwrap_or(directory).to_path_buf()
    } else {
        directory.to_path_buf()
    })
}

/// Parses the supported Matroska XML tag subset.
pub fn parse_matroska_tags(input: &str) -> Result<MatroskaEpisodeTags, LocalError> {
    let dto: TagsXml = quick_xml::de::from_str(input)?;
    let mut result = MatroskaEpisodeTags::default();
    for simple in dto.tags.into_iter().flat_map(|tag| tag.simple) {
        let value = simple.value.trim();
        if value.is_empty() {
            continue;
        }
        match simple.name.trim().to_ascii_uppercase().as_str() {
            "TVSHOW" | "SERIES" | "SHOW" => result.series_title = Some(value.to_owned()),
            "TITLE" | "EPISODE_TITLE" => result.episode_title = Some(value.to_owned()),
            "SEASON" | "SEASON_NUMBER" => result.season = value.parse().ok(),
            "EPISODE" | "EPISODE_NUMBER" => result.episode = value.parse().ok(),
            "ORDERING" | "ORDERING_SCHEME" => {
                result.ordering = match value.to_ascii_lowercase().as_str() {
                    "aired" => Some(OrderingScheme::Aired),
                    "dvd" => Some(OrderingScheme::Dvd),
                    "absolute" => Some(OrderingScheme::Absolute),
                    _ => None,
                }
            }
            "TMDB" | "TMDBID" | "TMDB_ID" => {
                result.external_ids.push(ExternalId::new("tmdb", value)?);
            }
            "TVDB" | "TVDBID" | "TVDB_ID" => {
                result.external_ids.push(ExternalId::new("tvdb", value)?);
            }
            "IMDB" | "IMDBID" | "IMDB_ID" => {
                result.external_ids.push(ExternalId::new("imdb", value)?);
            }
            _ => {}
        }
    }
    Ok(result)
}

/// Recursively scans local episode files without following symbolic links.
pub fn scan_television(root: &Path) -> Result<TelevisionScanResult, LocalError> {
    let (records, warnings) = scan_television_records(root)?;
    let mut documents = Vec::with_capacity(records.len());
    let mut roots = Vec::with_capacity(records.len());
    for record in records {
        documents.push(record.value);
        roots.push(record.root);
    }
    Ok(TelevisionScanResult {
        documents,
        roots,
        warnings,
    })
}

pub fn scan_television_records(
    root: &Path,
) -> Result<(Vec<TelevisionRecord>, Vec<ScanWarning>), LocalError> {
    if !root.is_dir() {
        return Err(LocalError::InvalidPath(root.to_path_buf()));
    }
    let mut paths = Vec::new();
    collect_episode_paths(root, &mut paths)?;
    paths.sort();
    let mut warnings = Vec::new();
    let mut groups = BTreeMap::<String, SeriesAggregate>::new();
    for path in paths {
        let Ok(mut hint) = identify_episode_path(&path) else {
            continue;
        };
        let mut ordering = hint.sequence.scheme;
        let sidecar = path.with_extension("tags.xml");
        if sidecar.is_file() {
            match fs::read_to_string(&sidecar)
                .map_err(LocalError::from)
                .and_then(|input| parse_matroska_tags(&input))
            {
                Ok(tags) => {
                    if let Some(title) = tags.series_title {
                        hint.series_title = title;
                    }
                    if let Some(title) = tags.episode_title {
                        hint.episode_title = Some(title);
                    }
                    ordering = tags.ordering.unwrap_or(ordering);
                    let season = tags.season.or(hint.sequence.season);
                    let episode = tags.episode.unwrap_or(hint.sequence.episode);
                    hint.sequence = episode_sequence(ordering, season, episode)?;
                    hint.external_ids.extend(tags.external_ids);
                }
                Err(error) => warnings.push(ScanWarning {
                    path: sidecar,
                    message: error.to_string(),
                }),
            }
        }
        let series_root = episode_series_root(&path)?;
        let key = format!(
            "{}\0{}",
            series_root.to_string_lossy(),
            normalize_key(&hint.series_title)
        );
        let aggregate = groups.entry(key).or_insert_with(|| SeriesAggregate {
            title: hint.series_title.clone(),
            root: series_root,
            ordering,
            external_ids: Vec::new(),
            episodes: Vec::new(),
        });
        if aggregate.ordering != ordering {
            return Err(LocalError::InvalidMetadata(format!(
                "mixed television ordering schemes for `{}`: {:?} and {:?}",
                aggregate.title, aggregate.ordering, ordering
            )));
        }
        for id in &hint.external_ids {
            if !aggregate.external_ids.contains(id) {
                aggregate.external_ids.push(id.clone());
            }
        }
        aggregate.episodes.push(hint);
    }
    let records = groups
        .into_values()
        .map(build_record)
        .collect::<Result<Vec<_>, _>>()?;
    Ok((records, warnings))
}

#[derive(Debug)]
struct SeriesAggregate {
    title: String,
    root: PathBuf,
    ordering: OrderingScheme,
    external_ids: Vec<ExternalId>,
    episodes: Vec<EpisodeHint>,
}

fn build_record(mut aggregate: SeriesAggregate) -> Result<TelevisionRecord, LocalError> {
    aggregate.episodes.sort_by_key(|hint| {
        (
            hint.sequence.season.unwrap_or_default(),
            hint.sequence.episode,
        )
    });
    let slug = slug(&aggregate.title);
    let mut by_season = BTreeMap::<u32, Vec<Episode>>::new();
    for hint in aggregate.episodes {
        let season_number = hint.sequence.season.unwrap_or_default();
        let episode_title = hint
            .episode_title
            .unwrap_or_else(|| format!("Episode {}", hint.sequence.episode));
        let mut episode_titles = LocalizedValue::new();
        episode_titles.insert("und", episode_title)?;
        let episode = Episode::new(
            WorkId::new(format!(
                "local-{slug}-s{season_number}-e{}",
                hint.sequence.episode
            ))?,
            episode_titles,
            hint.sequence,
        );
        by_season.entry(season_number).or_default().push(episode);
    }
    let seasons = by_season
        .into_iter()
        .map(|(number, episodes)| {
            Season::new(
                WorkId::new(format!("local-{slug}-season-{number}"))?,
                number,
                episodes,
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut series_titles = LocalizedValue::new();
    series_titles.insert("und", aggregate.title)?;
    let series = Series::new(
        WorkId::new(format!("local-{slug}"))?,
        series_titles,
        aggregate.ordering,
        seasons,
    );
    let external_id = aggregate
        .external_ids
        .into_iter()
        .next()
        .unwrap_or(ExternalId::new("local", series.id.as_str())?);
    Ok(TelevisionRecord {
        external_id,
        value: series,
        root: aggregate.root,
    })
}

fn collect_episode_paths(directory: &Path, paths: &mut Vec<PathBuf>) -> Result<(), LocalError> {
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path)?;
        if metadata.file_type().is_symlink() {
            continue;
        }
        if metadata.is_dir() {
            collect_episode_paths(&path, paths)?;
        } else if is_video(&path) {
            paths.push(path);
        }
    }
    Ok(())
}

fn is_video(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|value| value.to_str())
            .map(str::to_ascii_lowercase)
            .as_deref(),
        Some("mkv" | "mp4" | "m4v" | "avi" | "mov")
    )
}

fn episode_sequence(
    ordering: OrderingScheme,
    season: Option<u32>,
    episode: u32,
) -> Result<EpisodeSequence, LocalError> {
    match ordering {
        OrderingScheme::Aired => Ok(EpisodeSequence::aired(season.unwrap_or_default(), episode)?),
        OrderingScheme::Absolute => Ok(EpisodeSequence::absolute(episode)?),
        OrderingScheme::Dvd => {
            if episode == 0 {
                return Err(LocalError::InvalidMetadata(
                    "DVD episode number must be positive".to_owned(),
                ));
            }
            Ok(EpisodeSequence {
                scheme: OrderingScheme::Dvd,
                season,
                episode,
            })
        }
    }
}

fn parse_sxe(value: &str) -> Option<(usize, usize, u32, u32)> {
    let bytes = value.as_bytes();
    for start in 0..bytes.len() {
        if !matches!(bytes[start], b's' | b'S') {
            continue;
        }
        let mut cursor = start + 1;
        let season_start = cursor;
        while cursor < bytes.len() && bytes[cursor].is_ascii_digit() {
            cursor += 1;
        }
        if cursor == season_start || cursor >= bytes.len() || !matches!(bytes[cursor], b'e' | b'E')
        {
            continue;
        }
        let season = value[season_start..cursor].parse().ok()?;
        cursor += 1;
        let episode_start = cursor;
        while cursor < bytes.len() && bytes[cursor].is_ascii_digit() {
            cursor += 1;
        }
        if cursor == episode_start {
            continue;
        }
        let episode = value[episode_start..cursor].parse().ok()?;
        if episode > 0 {
            return Some((start, cursor, season, episode));
        }
    }
    None
}

fn parse_season_folder(value: &str) -> Option<u32> {
    let normalized = value.trim().to_ascii_lowercase();
    if matches!(normalized.as_str(), "special" | "specials") {
        return Some(0);
    }
    normalized
        .strip_prefix("season")
        .map(str::trim)
        .and_then(|number| number.parse().ok())
}

fn parse_episode_prefix(value: &str) -> Option<(u32, String)> {
    let trimmed = value.trim_start();
    let digits = trimmed.chars().take_while(char::is_ascii_digit).count();
    if digits == 0 {
        return None;
    }
    let episode = trimmed[..digits].parse::<u32>().ok()?;
    if episode == 0 {
        return None;
    }
    let remainder = clean_name(trimmed[digits..].trim_start_matches([' ', '-', '.', '_']));
    Some((episode, remainder))
}

fn series_folder_title(path: &Path, has_season_folder: bool) -> Result<String, LocalError> {
    let directory = path
        .parent()
        .ok_or_else(|| LocalError::InvalidPath(path.to_path_buf()))?;
    let series_directory = if has_season_folder {
        directory.parent().unwrap_or(directory)
    } else {
        directory
    };
    let title = series_directory
        .file_name()
        .and_then(|value| value.to_str())
        .map(clean_name)
        .ok_or_else(|| LocalError::Unidentified(path.to_path_buf()))?;
    if title.is_empty() {
        Err(LocalError::Unidentified(path.to_path_buf()))
    } else {
        Ok(title)
    }
}

fn parse_external_ids(path: &Path) -> Vec<ExternalId> {
    let value = path.to_string_lossy();
    let mut result = Vec::new();
    for (open, close) in [('{', '}'), ('[', ']')] {
        let mut remainder = value.as_ref();
        while let Some(start) = remainder.find(open) {
            let after = &remainder[start + 1..];
            let Some(end) = after.find(close) else {
                break;
            };
            let token = &after[..end];
            if let Some((namespace, id)) = token.split_once(':').or_else(|| token.split_once('-')) {
                if let Ok(external_id) = ExternalId::new(namespace.trim(), id.trim()) {
                    if !result.contains(&external_id) {
                        result.push(external_id);
                    }
                }
            }
            remainder = &after[end + 1..];
        }
    }
    result
}

fn clean_episode_suffix(value: &str) -> String {
    let suffix = value
        .split(['{', '['])
        .next()
        .unwrap_or(value)
        .trim_matches([' ', '.', '_', '-']);
    clean_name(suffix)
}

fn clean_name(value: &str) -> String {
    value
        .replace(['.', '_'], " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim_matches([' ', '-'])
        .to_owned()
}

fn normalize_key(value: &str) -> String {
    clean_name(value).to_lowercase()
}

fn slug(value: &str) -> String {
    let mut result = String::new();
    let mut separator = false;
    for ch in value.chars().flat_map(char::to_lowercase) {
        if ch.is_alphanumeric() {
            if separator && !result.is_empty() {
                result.push('-');
            }
            result.push(ch);
            separator = false;
        } else {
            separator = true;
        }
    }
    if result.is_empty() {
        "series".to_owned()
    } else {
        result
    }
}
