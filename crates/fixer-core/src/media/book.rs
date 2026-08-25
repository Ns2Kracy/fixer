//! Book works, editions, ISBNs, contributors, and file assets.

use super::common::{Asset, Credit, ReleaseId, Titles, WorkId, validate_text};
use crate::CoreError;
use serde::{Deserialize, Serialize};

/// Validated ISBN-10.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Isbn10(String);
impl Isbn10 {
    /// Parses digits, allowing `X` only as the check digit.
    pub fn new(value: impl Into<String>) -> Result<Self, CoreError> {
        let value = compact(value.into());
        let valid_shape = value.len() == 10
            && value
                .chars()
                .enumerate()
                .all(|(i, ch)| ch.is_ascii_digit() || (i == 9 && ch == 'X'));
        let sum: u32 = value
            .chars()
            .zip((1_u32..=10).rev())
            .map(|(ch, weight)| ch.to_digit(10).unwrap_or(10) * weight)
            .sum();
        if valid_shape && sum % 11 == 0 {
            Ok(Self(value))
        } else {
            Err(invalid("isbn_10", value))
        }
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Validated ISBN-13.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Isbn13(String);
impl Isbn13 {
    /// Parses and validates an ISBN-13 check digit.
    pub fn new(value: impl Into<String>) -> Result<Self, CoreError> {
        let value = compact(value.into());
        let digits = value
            .chars()
            .map(|ch| ch.to_digit(10))
            .collect::<Option<Vec<_>>>();
        let valid = digits.as_ref().is_some_and(|digits| {
            digits.len() == 13
                && digits
                    .iter()
                    .enumerate()
                    .map(|(i, digit)| digit * if i % 2 == 0 { 1 } else { 3 })
                    .sum::<u32>()
                    % 10
                    == 0
        });
        if valid {
            Ok(Self(value))
        } else {
            Err(invalid("isbn_13", value))
        }
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

fn compact(value: String) -> String {
    value
        .chars()
        .filter(|ch| !matches!(ch, '-' | ' '))
        .collect::<String>()
        .to_ascii_uppercase()
}
fn invalid(field: &'static str, value: String) -> CoreError {
    CoreError::InvalidDomainValue { field, value }
}

/// A specific published edition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BookEdition {
    pub id: ReleaseId,
    pub isbn_10: Isbn10,
    pub isbn_13: Isbn13,
    pub publisher: String,
    pub assets: Vec<Asset>,
}

impl BookEdition {
    /// Constructs a book edition.
    pub fn new(
        id: ReleaseId,
        isbn_10: Isbn10,
        isbn_13: Isbn13,
        publisher: impl Into<String>,
        assets: Vec<Asset>,
    ) -> Result<Self, CoreError> {
        let publisher = publisher.into();
        validate_text("book.publisher", &publisher, 512)?;
        Ok(Self {
            id,
            isbn_10,
            isbn_13,
            publisher,
            assets,
        })
    }
}

/// An abstract book work with contributors and editions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BookWork {
    pub id: WorkId,
    pub titles: Titles,
    pub contributors: Vec<Credit>,
    pub editions: Vec<BookEdition>,
}
impl BookWork {
    /// Constructs a book work.
    pub const fn new(
        id: WorkId,
        titles: Titles,
        contributors: Vec<Credit>,
        editions: Vec<BookEdition>,
    ) -> Self {
        Self {
            id,
            titles,
            contributors,
            editions,
        }
    }
}
