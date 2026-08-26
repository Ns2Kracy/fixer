//! Read-only EPUB container and OPF metadata parsing.

use crate::LocalError;
use fixer_core::{Isbn10, Isbn13};
use quick_xml::{Reader, XmlVersion, events::Event};
use std::{
    io::{Cursor, Read},
    path::{Component, Path},
};
use zip::ZipArchive;

const MAX_METADATA_BYTES: u64 = 1024 * 1024;

/// Selected baseline EPUB package metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BookMetadata {
    pub title: String,
    pub authors: Vec<String>,
    pub publisher: Option<String>,
    pub isbn_10: Option<Isbn10>,
    pub isbn_13: Option<Isbn13>,
}

/// Parses selected Dublin Core fields from an OPF package document.
pub fn parse_opf(input: &str) -> Result<BookMetadata, LocalError> {
    let mut reader = Reader::from_str(input);
    reader.config_mut().trim_text(true);
    let mut in_metadata = false;
    let mut current_field: Option<Field> = None;
    let mut title = None;
    let mut authors = Vec::new();
    let mut publisher = None;
    let mut identifiers = Vec::new();

    loop {
        match reader.read_event() {
            Ok(Event::Start(element)) => {
                let name = element.local_name();
                match name.as_ref() {
                    b"metadata" => in_metadata = true,
                    b"title" if in_metadata => current_field = Some(Field::Title),
                    b"creator" if in_metadata => current_field = Some(Field::Creator),
                    b"publisher" if in_metadata => current_field = Some(Field::Publisher),
                    b"identifier" if in_metadata => current_field = Some(Field::Identifier),
                    _ => {}
                }
            }
            Ok(Event::Text(text)) if in_metadata => {
                let value = text
                    .decode()
                    .map_err(|error| metadata_error(error.to_string()))?
                    .trim()
                    .to_owned();
                if value.is_empty() {
                    continue;
                }
                match current_field {
                    Some(Field::Title) if title.is_none() => title = Some(value),
                    Some(Field::Creator) => authors.push(value),
                    Some(Field::Publisher) if publisher.is_none() => publisher = Some(value),
                    Some(Field::Identifier) => identifiers.push(value),
                    _ => {}
                }
            }
            Ok(Event::End(element)) => match element.local_name().as_ref() {
                b"metadata" => {
                    in_metadata = false;
                    current_field = None;
                }
                b"title" | b"creator" | b"publisher" | b"identifier" => {
                    current_field = None;
                }
                _ => {}
            },
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(error) => return Err(metadata_error(format!("invalid OPF XML: {error}"))),
        }
    }

    let title = title
        .filter(|value: &String| !value.trim().is_empty())
        .ok_or_else(|| metadata_error("OPF title is required"))?;
    authors.retain(|author| !author.trim().is_empty());
    if authors.is_empty() {
        return Err(metadata_error("OPF author is required"));
    }
    let (isbn_10, isbn_13) = canonical_isbns(&identifiers)?;
    Ok(BookMetadata {
        title,
        authors,
        publisher,
        isbn_10: Some(isbn_10),
        isbn_13: Some(isbn_13),
    })
}

/// Parses an EPUB byte slice without modifying it or accessing the filesystem.
pub fn parse_epub(bytes: &[u8]) -> Result<BookMetadata, LocalError> {
    let mut archive = ZipArchive::new(Cursor::new(bytes))?;
    let container = read_entry(&mut archive, "META-INF/container.xml")?;
    let rootfile = parse_rootfile(&container)?;
    validate_rootfile(&rootfile)?;
    let package = read_entry(&mut archive, &rootfile)?;
    parse_opf(&package)
}

#[derive(Debug, Clone, Copy)]
enum Field {
    Title,
    Creator,
    Publisher,
    Identifier,
}

fn parse_rootfile(input: &str) -> Result<String, LocalError> {
    let mut reader = Reader::from_str(input);
    reader.config_mut().trim_text(true);
    loop {
        match reader.read_event() {
            Ok(Event::Empty(element)) | Ok(Event::Start(element))
                if element.local_name().as_ref() == b"rootfile" =>
            {
                for attribute in element.attributes() {
                    let attribute = attribute.map_err(|error| {
                        metadata_error(format!("invalid container XML: {error}"))
                    })?;
                    if attribute.key.local_name().as_ref() == b"full-path" {
                        let value = attribute
                            .decoded_and_normalized_value(XmlVersion::Implicit1_0, reader.decoder())
                            .map_err(|error| {
                                metadata_error(format!("invalid rootfile path: {error}"))
                            })?
                            .into_owned();
                        if !value.trim().is_empty() {
                            return Ok(value);
                        }
                    }
                }
            }
            Ok(Event::Eof) => {
                return Err(metadata_error("EPUB container has no rootfile"));
            }
            Ok(_) => {}
            Err(error) => {
                return Err(metadata_error(format!("invalid container XML: {error}")));
            }
        }
    }
}

fn validate_rootfile(value: &str) -> Result<(), LocalError> {
    let path = Path::new(value);
    let safe = !path.is_absolute()
        && !value.contains('\\')
        && path
            .components()
            .all(|component| matches!(component, Component::Normal(_)));
    if safe {
        Ok(())
    } else {
        Err(metadata_error("EPUB rootfile path is unsafe"))
    }
}

fn read_entry(archive: &mut ZipArchive<Cursor<&[u8]>>, name: &str) -> Result<String, LocalError> {
    let entry = archive.by_name(name)?;
    if entry.size() > MAX_METADATA_BYTES {
        return Err(metadata_error(format!(
            "EPUB metadata entry `{name}` exceeds {MAX_METADATA_BYTES} bytes"
        )));
    }
    let mut bytes = Vec::with_capacity(usize::try_from(entry.size()).unwrap_or_default());
    entry.take(MAX_METADATA_BYTES + 1).read_to_end(&mut bytes)?;
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > MAX_METADATA_BYTES {
        return Err(metadata_error(format!(
            "EPUB metadata entry `{name}` exceeds {MAX_METADATA_BYTES} bytes"
        )));
    }
    String::from_utf8(bytes).map_err(|error| {
        metadata_error(format!(
            "EPUB metadata entry `{name}` is not UTF-8: {error}"
        ))
    })
}

fn canonical_isbns(values: &[String]) -> Result<(Isbn10, Isbn13), LocalError> {
    let mut isbn_10 = None;
    let mut isbn_13 = None;
    for value in values {
        let candidate = value
            .trim()
            .strip_prefix("urn:isbn:")
            .or_else(|| value.trim().strip_prefix("URN:ISBN:"))
            .unwrap_or(value.trim());
        if isbn_13.is_none() {
            isbn_13 = Isbn13::new(candidate.to_owned()).ok();
        }
        if isbn_10.is_none() {
            isbn_10 = Isbn10::new(candidate.to_owned()).ok();
        }
    }
    match (isbn_10, isbn_13) {
        (Some(isbn_10), Some(isbn_13)) => Ok((isbn_10, isbn_13)),
        (Some(isbn_10), None) => {
            let isbn_13 = isbn13_from_isbn10(&isbn_10)?;
            Ok((isbn_10, isbn_13))
        }
        (None, Some(isbn_13)) => {
            let isbn_10 = isbn10_from_isbn13(&isbn_13)?;
            Ok((isbn_10, isbn_13))
        }
        (None, None) => Err(metadata_error("OPF requires a valid ISBN-10 or ISBN-13")),
    }
}

fn isbn13_from_isbn10(isbn: &Isbn10) -> Result<Isbn13, LocalError> {
    let body = format!("978{}", &isbn.as_str()[..9]);
    let sum = body
        .bytes()
        .enumerate()
        .map(|(index, byte)| u32::from(byte - b'0') * if index % 2 == 0 { 1 } else { 3 })
        .sum::<u32>();
    let check = (10 - sum % 10) % 10;
    Isbn13::new(format!("{body}{check}")).map_err(LocalError::from)
}

fn isbn10_from_isbn13(isbn: &Isbn13) -> Result<Isbn10, LocalError> {
    let value = isbn.as_str();
    if !value.starts_with("978") {
        return Err(metadata_error(
            "ISBN-13 cannot be represented as ISBN-10 unless it starts with 978",
        ));
    }
    let body = &value[3..12];
    let sum = body
        .bytes()
        .zip((2_u32..=10).rev())
        .map(|(byte, weight)| u32::from(byte - b'0') * weight)
        .sum::<u32>();
    let check = (11 - sum % 11) % 11;
    let suffix = if check == 10 {
        "X".to_owned()
    } else {
        check.to_string()
    };
    Isbn10::new(format!("{body}{suffix}")).map_err(LocalError::from)
}

fn metadata_error(message: impl Into<String>) -> LocalError {
    LocalError::InvalidMetadata(message.into())
}
