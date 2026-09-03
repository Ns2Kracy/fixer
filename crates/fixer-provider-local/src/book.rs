//! Read-only EPUB container and OPF metadata parsing.

use crate::LocalError;
use fixer_core::{
    Asset, AssetId, AssetKind, BookEdition, BookWork, Credit, CreditRole, ExternalId, Isbn10,
    Isbn13, LocalizedValue, Person, PersonId, ReleaseId, SourcePath, WorkId,
};
use quick_xml::{Reader, XmlVersion, events::Event};
use std::{
    collections::BTreeMap,
    fs,
    io::{Cursor, Read},
    path::{Component, Path, PathBuf},
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
            Ok(Event::Empty(element) | Event::Start(element))
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
            .unwrap_or_else(|| value.trim());
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

/// Documents, work roots, exact ISBN records, and warnings produced by a book scan.
#[derive(Debug, Clone, Default)]
pub struct BookScanResult {
    pub documents: Vec<BookWork>,
    pub roots: Vec<PathBuf>,
    pub warnings: Vec<crate::ScanWarning>,
    pub(crate) records: Vec<(ExternalId, BookWork)>,
}

#[derive(Debug)]
struct WorkAggregate {
    title: String,
    authors: Vec<String>,
    root: PathBuf,
    editions: Vec<BookEdition>,
}

/// Recursively reads EPUB metadata without following symbolic links.
pub fn scan_books(root: &Path) -> Result<BookScanResult, LocalError> {
    if !root.is_dir() {
        return Err(LocalError::InvalidPath(root.to_path_buf()));
    }
    let mut paths = Vec::new();
    collect_epubs(root, &mut paths)?;
    paths.sort();
    let mut groups = BTreeMap::<String, WorkAggregate>::new();
    let mut warnings = Vec::new();
    for path in paths {
        match fs::read(&path)
            .map_err(LocalError::from)
            .and_then(|bytes| parse_epub(&bytes))
            .and_then(|metadata| add_edition(&mut groups, &path, metadata))
        {
            Ok(()) => {}
            Err(error) => warnings.push(crate::ScanWarning {
                path,
                message: error.to_string(),
            }),
        }
    }
    let mut documents = Vec::with_capacity(groups.len());
    let mut roots = Vec::with_capacity(groups.len());
    let mut records = Vec::new();
    for aggregate in groups.into_values() {
        let edition_isbns = aggregate
            .editions
            .iter()
            .map(|edition| edition.isbn_13.as_str().to_owned())
            .collect::<Vec<_>>();
        roots.push(aggregate.root.clone());
        let document = build_work(aggregate)?;
        for isbn in edition_isbns {
            records.push((ExternalId::new("isbn", isbn)?, document.clone()));
        }
        documents.push(document);
    }
    Ok(BookScanResult {
        documents,
        roots,
        warnings,
        records,
    })
}

fn add_edition(
    groups: &mut BTreeMap<String, WorkAggregate>,
    path: &Path,
    metadata: BookMetadata,
) -> Result<(), LocalError> {
    let root = path
        .parent()
        .ok_or_else(|| LocalError::InvalidPath(path.to_path_buf()))?
        .to_path_buf();
    let isbn_10 = metadata
        .isbn_10
        .ok_or_else(|| metadata_error("EPUB has no canonical ISBN-10"))?;
    let isbn_13 = metadata
        .isbn_13
        .ok_or_else(|| metadata_error("EPUB has no canonical ISBN-13"))?;
    let isbn = isbn_13.as_str().to_owned();
    let publisher = metadata
        .publisher
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "Unknown publisher".to_owned());
    let asset = Asset::new(
        AssetId::new(format!("local-book-file-{isbn}"))?,
        SourcePath::new(path.to_string_lossy().into_owned())?,
        AssetKind::BookFile,
    );
    let edition = BookEdition::new(
        ReleaseId::new(format!("local-book-edition-{isbn}"))?,
        isbn_10,
        isbn_13,
        publisher,
        vec![asset],
    )?;
    let key = format!(
        "{}\0{}\0{}",
        root.to_string_lossy(),
        normalize(&metadata.title),
        metadata
            .authors
            .iter()
            .map(|author| normalize(author))
            .collect::<Vec<_>>()
            .join("\0")
    );
    let aggregate = groups.entry(key).or_insert_with(|| WorkAggregate {
        title: metadata.title,
        authors: metadata.authors,
        root,
        editions: Vec::new(),
    });
    if aggregate
        .editions
        .iter()
        .any(|existing| existing.isbn_13.as_str() == isbn)
    {
        return Err(metadata_error(format!("duplicate EPUB ISBN `{isbn}`")));
    }
    aggregate.editions.push(edition);
    Ok(())
}

fn build_work(mut aggregate: WorkAggregate) -> Result<BookWork, LocalError> {
    aggregate
        .editions
        .sort_by(|left, right| left.isbn_13.as_str().cmp(right.isbn_13.as_str()));
    let work_slug = slug(&format!(
        "{}-{}",
        aggregate.title,
        aggregate.authors.join("-")
    ));
    let mut titles = LocalizedValue::new();
    titles.insert("und", aggregate.title)?;
    let contributors = aggregate
        .authors
        .into_iter()
        .map(|author| {
            let person = Person::new(
                PersonId::new(format!("local-book-author-{}", slug(&author)))?,
                author,
            )?;
            Ok(Credit::new(person, CreditRole::Author))
        })
        .collect::<Result<Vec<_>, LocalError>>()?;
    Ok(BookWork::new(
        WorkId::new(format!("local-book-work-{work_slug}"))?,
        titles,
        contributors,
        aggregate.editions,
    ))
}

fn collect_epubs(directory: &Path, paths: &mut Vec<PathBuf>) -> Result<(), LocalError> {
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path)?;
        if metadata.file_type().is_symlink() {
            continue;
        }
        if metadata.is_dir() {
            collect_epubs(&path, paths)?;
        } else if path
            .extension()
            .and_then(|value| value.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("epub"))
        {
            paths.push(path);
        }
    }
    Ok(())
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
