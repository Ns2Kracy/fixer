//! Movie hints derived from local path names.

use crate::LocalError;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Origin of one local identification observation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceKind {
    Filename,
    Directory,
    Year,
}

/// One observation supporting a local media hint.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HintEvidence {
    pub kind: EvidenceKind,
    pub value: String,
}

/// Evidence-bearing movie title and year inferred from a path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MediaHint {
    pub title: String,
    pub year: Option<u16>,
    pub evidence: Vec<HintEvidence>,
}

/// Identifies a movie from a file path without accessing the filesystem.
pub fn identify_path(path: &Path) -> Result<MediaHint, LocalError> {
    let stem = path
        .file_stem()
        .and_then(|value| value.to_str())
        .ok_or_else(|| LocalError::InvalidPath(path.to_path_buf()))?;
    let parent = path
        .parent()
        .and_then(Path::file_name)
        .and_then(|value| value.to_str());
    let generic = matches!(
        stem.to_ascii_lowercase().as_str(),
        "movie" | "video" | "film"
    );
    let (raw, kind) = if generic {
        parent.map_or((stem, EvidenceKind::Filename), |value| {
            (value, EvidenceKind::Directory)
        })
    } else {
        (stem, EvidenceKind::Filename)
    };
    let (title, year) = parse_name(raw);
    if title.trim().is_empty() {
        return Err(LocalError::Unidentified(path.to_path_buf()));
    }
    let mut evidence = vec![HintEvidence {
        kind,
        value: raw.to_owned(),
    }];
    if let Some(year) = year {
        evidence.push(HintEvidence {
            kind: EvidenceKind::Year,
            value: year.to_string(),
        });
    }
    Ok(MediaHint {
        title,
        year,
        evidence,
    })
}

fn parse_name(raw: &str) -> (String, Option<u16>) {
    let normalized = raw.replace(['.', '_'], " ");
    let tokens = normalized.split_whitespace().collect::<Vec<_>>();
    let year_index = tokens.iter().position(|token| parse_year(token).is_some());
    if let Some(index) = year_index {
        let year = parse_year(tokens[index]);
        let title = clean_title(&tokens[..index].join(" "));
        return (title, year);
    }
    let (without_year, year) = parenthesized_year(&normalized);
    (clean_title(&without_year), year)
}

fn parse_year(token: &str) -> Option<u16> {
    let token = token.trim_matches(['(', ')', '[', ']']);
    if token.len() != 4 || !token.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let year = token.parse::<u16>().ok()?;
    (1870..=2100).contains(&year).then_some(year)
}

fn parenthesized_year(value: &str) -> (String, Option<u16>) {
    for open in ['(', '['] {
        let close = if open == '(' { ')' } else { ']' };
        if let Some(start) = value.rfind(open) {
            if value.ends_with(close) {
                if let Some(year) = parse_year(&value[start + 1..value.len() - 1]) {
                    return (value[..start].trim().to_owned(), Some(year));
                }
            }
        }
    }
    (value.to_owned(), None)
}

fn clean_title(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}
