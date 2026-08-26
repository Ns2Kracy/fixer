//! Read-only baseline music metadata parsers.

use crate::{LocalError, ScanWarning};
use fixer_core::{
    AssetId, Disc, Duration, LocalizedValue, MusicArtist, MusicRelease, MusicReleaseGroup,
    ReleaseId, Track, TrackSequence, WorkId,
};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

const ID3V1_SIZE: usize = 128;
const CUE_FRAMES_PER_SECOND: u32 = 75;

/// Baseline ID3v1 or ID3v1.1 fields read from an MP3 tail.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Id3v1Tags {
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub year: Option<u16>,
    pub comment: Option<String>,
    pub track: Option<u8>,
    pub genre: u8,
}

/// One file declaration and its tracks from a CUE sheet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CueFile {
    pub path: String,
    pub tracks: Vec<CueTrack>,
}

/// One audio track from a CUE sheet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CueTrack {
    pub number: u32,
    pub title: Option<String>,
    pub performer: Option<String>,
    /// `INDEX 01` position in 75 Hz CD frames.
    pub index_frames: Option<u32>,
}

/// Global album metadata and file hierarchy from a CUE sheet.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CueSheet {
    pub title: Option<String>,
    pub performer: Option<String>,
    pub files: Vec<CueFile>,
}

/// Parses an ID3v1/1.1 tag from complete MP3 bytes without mutating them.
///
/// Returns `Ok(None)` when a complete trailing tag block is present but has no
/// `TAG` signature. A leading `TAG` in an undersized input is reported as
/// truncated metadata rather than silently ignored.
pub fn parse_id3v1(bytes: &[u8]) -> Result<Option<Id3v1Tags>, LocalError> {
    if bytes.len() < ID3V1_SIZE {
        return if bytes.starts_with(b"TAG") {
            Err(LocalError::InvalidMetadata(
                "truncated ID3v1 tag".to_owned(),
            ))
        } else {
            Ok(None)
        };
    }
    let tag = &bytes[bytes.len() - ID3V1_SIZE..];
    if &tag[..3] != b"TAG" {
        return Ok(None);
    }
    let id3v11 = tag[125] == 0 && tag[126] != 0;
    let comment_end = if id3v11 { 125 } else { 127 };
    let year_text = decode_text(&tag[93..97]);
    let year = year_text
        .as_deref()
        .filter(|value| value.len() == 4 && value.bytes().all(|byte| byte.is_ascii_digit()))
        .and_then(|value| value.parse().ok());
    Ok(Some(Id3v1Tags {
        title: decode_text(&tag[3..33]),
        artist: decode_text(&tag[33..63]),
        album: decode_text(&tag[63..93]),
        year,
        comment: decode_text(&tag[97..comment_end]),
        track: id3v11.then_some(tag[126]),
        genre: tag[127],
    }))
}

/// Parses the baseline UTF-8 CUE subset used for album and track identity.
pub fn parse_cue(input: &str) -> Result<CueSheet, LocalError> {
    let mut sheet = CueSheet::default();
    let mut current_file: Option<usize> = None;
    let mut current_track: Option<usize> = None;

    for (line_index, raw_line) in input.lines().enumerate() {
        let line_number = line_index + 1;
        let line = raw_line.trim().trim_start_matches('\u{feff}').trim();
        if line.is_empty() || line.starts_with("REM ") {
            continue;
        }
        let (command, rest) = split_command(line);
        match command {
            "FILE" => {
                let path = first_argument(rest)
                    .ok_or_else(|| cue_error(line_number, "FILE path is required"))?;
                if path.trim().is_empty() {
                    return Err(cue_error(line_number, "FILE path is required"));
                }
                sheet.files.push(CueFile {
                    path,
                    tracks: Vec::new(),
                });
                current_file = Some(sheet.files.len() - 1);
                current_track = None;
            }
            "TRACK" => {
                let file_index =
                    current_file.ok_or_else(|| cue_error(line_number, "TRACK must follow FILE"))?;
                let number = rest
                    .split_whitespace()
                    .next()
                    .and_then(|value| value.parse::<u32>().ok())
                    .filter(|number| *number > 0)
                    .ok_or_else(|| cue_error(line_number, "TRACK number must be positive"))?;
                sheet.files[file_index].tracks.push(CueTrack {
                    number,
                    title: None,
                    performer: None,
                    index_frames: None,
                });
                current_track = Some(sheet.files[file_index].tracks.len() - 1);
            }
            "TITLE" => {
                let value = first_argument(rest)
                    .filter(|value| !value.trim().is_empty())
                    .ok_or_else(|| cue_error(line_number, "TITLE value is required"))?;
                if let (Some(file_index), Some(track_index)) = (current_file, current_track) {
                    sheet.files[file_index].tracks[track_index].title = Some(value);
                } else {
                    sheet.title = Some(value);
                }
            }
            "PERFORMER" => {
                let value = first_argument(rest)
                    .filter(|value| !value.trim().is_empty())
                    .ok_or_else(|| cue_error(line_number, "PERFORMER value is required"))?;
                if let (Some(file_index), Some(track_index)) = (current_file, current_track) {
                    sheet.files[file_index].tracks[track_index].performer = Some(value);
                } else {
                    sheet.performer = Some(value);
                }
            }
            "INDEX" => {
                let Some(file_index) = current_file else {
                    return Err(cue_error(line_number, "INDEX must follow TRACK"));
                };
                let Some(track_index) = current_track else {
                    return Err(cue_error(line_number, "INDEX must follow TRACK"));
                };
                let mut fields = rest.split_whitespace();
                let index = fields.next().unwrap_or_default();
                let position = fields.next().unwrap_or_default();
                if index == "01" {
                    sheet.files[file_index].tracks[track_index].index_frames =
                        Some(parse_cue_position(position, line_number)?);
                }
            }
            _ => {}
        }
    }
    Ok(sheet)
}

fn decode_text(bytes: &[u8]) -> Option<String> {
    let end = bytes
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(bytes.len());
    let value = bytes[..end]
        .iter()
        .map(|byte| char::from(*byte))
        .collect::<String>()
        .trim_end()
        .to_owned();
    (!value.is_empty()).then_some(value)
}

fn split_command(line: &str) -> (&str, &str) {
    line.split_once(char::is_whitespace)
        .map_or((line, ""), |(command, rest)| (command, rest.trim()))
}

fn first_argument(value: &str) -> Option<String> {
    let value = value.trim();
    if let Some(quoted) = value.strip_prefix('"') {
        let end = quoted.find('"')?;
        Some(quoted[..end].to_owned())
    } else {
        value
            .split_whitespace()
            .next()
            .filter(|part| !part.is_empty())
            .map(str::to_owned)
    }
}

fn parse_cue_position(value: &str, line_number: usize) -> Result<u32, LocalError> {
    let mut fields = value.split(':');
    let minutes = parse_position_field(fields.next(), line_number)?;
    let seconds = parse_position_field(fields.next(), line_number)?;
    let frames = parse_position_field(fields.next(), line_number)?;
    if fields.next().is_some() || seconds >= 60 || frames >= CUE_FRAMES_PER_SECOND {
        return Err(cue_error(line_number, "invalid INDEX position"));
    }
    minutes
        .checked_mul(60)
        .and_then(|value| value.checked_add(seconds))
        .and_then(|value| value.checked_mul(CUE_FRAMES_PER_SECOND))
        .and_then(|value| value.checked_add(frames))
        .ok_or_else(|| cue_error(line_number, "INDEX position overflowed"))
}

fn parse_position_field(value: Option<&str>, line_number: usize) -> Result<u32, LocalError> {
    value
        .and_then(|field| field.parse().ok())
        .ok_or_else(|| cue_error(line_number, "invalid INDEX position"))
}

fn cue_error(line: usize, message: &str) -> LocalError {
    LocalError::InvalidMetadata(format!("CUE line {line}: {message}"))
}

/// Documents, album roots, and non-fatal warnings produced by a music scan.
#[derive(Debug, Clone, Default)]
pub struct MusicScanResult {
    pub documents: Vec<MusicReleaseGroup>,
    pub roots: Vec<PathBuf>,
    pub warnings: Vec<ScanWarning>,
}

#[derive(Debug)]
struct AlbumAggregate {
    artist: String,
    title: String,
    root: PathBuf,
    discs: BTreeMap<u32, Vec<TrackInput>>,
}

#[derive(Debug)]
struct TrackInput {
    number: u32,
    title: String,
    duration: Duration,
}

/// Recursively reads baseline ID3v1 and CUE metadata without following symlinks.
pub fn scan_music(root: &Path) -> Result<MusicScanResult, LocalError> {
    if !root.is_dir() {
        return Err(LocalError::InvalidPath(root.to_path_buf()));
    }
    let mut paths = Vec::new();
    collect_music_metadata(root, &mut paths)?;
    paths.sort();
    let mut groups = BTreeMap::<String, AlbumAggregate>::new();
    let mut warnings = Vec::new();
    for path in paths {
        let result = match path
            .extension()
            .and_then(|value| value.to_str())
            .map(str::to_ascii_lowercase)
            .as_deref()
        {
            Some("cue") => read_cue_album(&path, &mut groups),
            Some("mp3") => read_id3_track(&path, &mut groups),
            _ => Ok(()),
        };
        if let Err(error) = result {
            warnings.push(ScanWarning {
                path,
                message: error.to_string(),
            });
        }
    }
    let mut documents = Vec::with_capacity(groups.len());
    let mut roots = Vec::with_capacity(groups.len());
    for aggregate in groups.into_values() {
        roots.push(aggregate.root.clone());
        documents.push(build_album(aggregate)?);
    }
    Ok(MusicScanResult {
        documents,
        roots,
        warnings,
    })
}

fn read_cue_album(
    path: &Path,
    groups: &mut BTreeMap<String, AlbumAggregate>,
) -> Result<(), LocalError> {
    let sheet = parse_cue(&fs::read_to_string(path)?)?;
    let artist = required_metadata(sheet.performer, "CUE album performer is required")?;
    let title = required_metadata(sheet.title, "CUE album title is required")?;
    let directory = path
        .parent()
        .ok_or_else(|| LocalError::InvalidPath(path.to_path_buf()))?;
    let (root, disc_number) = album_root_and_disc(directory);
    let aggregate = album_group(groups, artist, title, root);
    let tracks = aggregate.discs.entry(disc_number).or_default();
    for file in sheet.files {
        for (index, track) in file.tracks.iter().enumerate() {
            let title = required_metadata(track.title.clone(), "CUE track title is required")?;
            let duration = track_duration(&file.tracks, index);
            tracks.push(TrackInput {
                number: track.number,
                title,
                duration,
            });
        }
    }
    Ok(())
}

fn read_id3_track(
    path: &Path,
    groups: &mut BTreeMap<String, AlbumAggregate>,
) -> Result<(), LocalError> {
    let Some(tags) = parse_id3v1(&fs::read(path)?)? else {
        return Ok(());
    };
    let artist = required_metadata(tags.artist, "ID3 artist is required")?;
    let album = required_metadata(tags.album, "ID3 album is required")?;
    let title = required_metadata(tags.title, "ID3 title is required")?;
    let number = tags
        .track
        .map(u32::from)
        .filter(|number| *number > 0)
        .ok_or_else(|| LocalError::InvalidMetadata("ID3 track number is required".to_owned()))?;
    let root = path
        .parent()
        .ok_or_else(|| LocalError::InvalidPath(path.to_path_buf()))?
        .to_path_buf();
    album_group(groups, artist, album, root)
        .discs
        .entry(1)
        .or_default()
        .push(TrackInput {
            number,
            title,
            duration: Duration::from_seconds(0),
        });
    Ok(())
}

fn album_group(
    groups: &mut BTreeMap<String, AlbumAggregate>,
    artist: String,
    title: String,
    root: PathBuf,
) -> &mut AlbumAggregate {
    let key = format!(
        "{}\0{}\0{}",
        root.to_string_lossy(),
        normalize(&artist),
        normalize(&title)
    );
    groups.entry(key).or_insert_with(|| AlbumAggregate {
        artist,
        title,
        root,
        discs: BTreeMap::new(),
    })
}

fn build_album(aggregate: AlbumAggregate) -> Result<MusicReleaseGroup, LocalError> {
    let album_slug = slug(&format!("{}-{}", aggregate.artist, aggregate.title));
    let mut discs = Vec::with_capacity(aggregate.discs.len());
    for (disc_number, mut inputs) in aggregate.discs {
        inputs.sort_by_key(|track| track.number);
        let tracks = inputs
            .into_iter()
            .map(|input| {
                let mut titles = LocalizedValue::new();
                titles.insert("und", input.title)?;
                Ok(Track::new(
                    AssetId::new(format!(
                        "local-music-{album_slug}-d{disc_number}-t{}",
                        input.number
                    ))?,
                    titles,
                    TrackSequence::new(disc_number, input.number)?,
                    input.duration,
                ))
            })
            .collect::<Result<Vec<_>, LocalError>>()?;
        discs.push(Disc::new(disc_number, tracks)?);
    }
    discs.sort_by_key(|disc| disc.number);
    let artist = MusicArtist::new(
        WorkId::new(format!("local-music-artist-{}", slug(&aggregate.artist)))?,
        aggregate.artist,
    )?;
    let mut titles = LocalizedValue::new();
    titles.insert("und", aggregate.title)?;
    Ok(MusicReleaseGroup::new(
        WorkId::new(format!("local-music-release-group-{album_slug}"))?,
        titles,
        artist,
        vec![MusicRelease::new(
            ReleaseId::new(format!("local-music-release-{album_slug}"))?,
            discs,
        )],
    ))
}

fn collect_music_metadata(directory: &Path, paths: &mut Vec<PathBuf>) -> Result<(), LocalError> {
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path)?;
        if metadata.file_type().is_symlink() {
            continue;
        }
        if metadata.is_dir() {
            collect_music_metadata(&path, paths)?;
        } else if matches!(
            path.extension()
                .and_then(|value| value.to_str())
                .map(str::to_ascii_lowercase)
                .as_deref(),
            Some("cue" | "mp3")
        ) {
            paths.push(path);
        }
    }
    Ok(())
}

fn album_root_and_disc(directory: &Path) -> (PathBuf, u32) {
    let name = directory
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    if let Some(number) = disc_folder_number(name) {
        return (
            directory.parent().unwrap_or(directory).to_path_buf(),
            number,
        );
    }
    (directory.to_path_buf(), 1)
}

fn disc_folder_number(value: &str) -> Option<u32> {
    let normalized = value.to_ascii_lowercase();
    ["disc", "disk", "cd"].into_iter().find_map(|prefix| {
        normalized
            .strip_prefix(prefix)
            .map(str::trim)
            .and_then(|number| number.parse().ok())
            .filter(|number| *number > 0)
    })
}

fn track_duration(tracks: &[CueTrack], index: usize) -> Duration {
    let start = tracks[index].index_frames;
    let end = tracks.get(index + 1).and_then(|track| track.index_frames);
    let seconds = start
        .zip(end)
        .and_then(|(start, end)| end.checked_sub(start))
        .map_or(0, |frames| u64::from(frames / CUE_FRAMES_PER_SECOND));
    Duration::from_seconds(seconds)
}

fn required_metadata(value: Option<String>, message: &str) -> Result<String, LocalError> {
    value
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| LocalError::InvalidMetadata(message.to_owned()))
}

fn normalize(value: &str) -> String {
    value.split_whitespace().collect::<String>().to_lowercase()
}

fn slug(value: &str) -> String {
    let mut output = value
        .chars()
        .map(|character| {
            if character.is_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>();
    while output.contains("--") {
        output = output.replace("--", "-");
    }
    output.trim_matches('-').to_owned()
}
