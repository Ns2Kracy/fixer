//! Read-only baseline music metadata parsers.

use crate::LocalError;

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
