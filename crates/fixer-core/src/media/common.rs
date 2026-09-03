//! Value objects shared across media domains.

use crate::{CoreError, ExternalId, LocalizedValue};
use serde::{Deserialize, Serialize};

macro_rules! string_id {
    ($(#[$meta:meta])* $name:ident, $field:literal) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            /// Constructs a non-empty identifier.
            pub fn new(value: impl Into<String>) -> Result<Self, CoreError> {
                let value = value.into();
                validate_text($field, &value, 256)?;
                Ok(Self(value))
            }

            /// Returns the identifier as text.
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }
    };
}

string_id!(/// Stable identity for an abstract work.
    WorkId, "work_id");
string_id!(/// Stable identity for a particular release or edition.
    ReleaseId, "release_id");
string_id!(/// Stable identity for a local or remote asset.
    AssetId, "asset_id");
string_id!(/// Stable identity for a person.
    PersonId, "person_id");

/// Localized titles for a work or release.
pub type Titles = LocalizedValue<String>;
/// Localized prose summaries.
pub type Summaries = LocalizedValue<String>;

/// A validated calendar date with optional month and day precision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ReleaseDate {
    /// Four-digit year.
    pub year: u16,
    /// Month when known.
    pub month: Option<u8>,
    /// Day when known.
    pub day: Option<u8>,
}

impl ReleaseDate {
    /// Constructs a year-precision date.
    pub fn year(year: u16) -> Result<Self, CoreError> {
        Self::validate(year, None, None)
    }

    /// Constructs a full calendar date.
    pub fn ymd(year: u16, month: u8, day: u8) -> Result<Self, CoreError> {
        Self::validate(year, Some(month), Some(day))
    }

    fn validate(year: u16, month: Option<u8>, day: Option<u8>) -> Result<Self, CoreError> {
        let valid_year = year > 0;
        let valid_month = month.is_none_or(|value| (1..=12).contains(&value));
        let valid_day = match (month, day) {
            (None, None) => true,
            (Some(month), Some(day)) => day > 0 && day <= days_in_month(year, month),
            _ => false,
        };
        if valid_year && valid_month && valid_day {
            Ok(Self { year, month, day })
        } else {
            Err(CoreError::InvalidDomainValue {
                field: "release_date",
                value: format!("{year}-{month:?}-{day:?}"),
            })
        }
    }
}

const fn days_in_month(year: u16, month: u8) -> u8 {
    match month {
        4 | 6 | 9 | 11 => 30,
        2 if year % 400 == 0 || (year % 4 == 0 && year % 100 != 0) => 29,
        2 => 28,
        _ => 31,
    }
}

/// A person credited on a work or release.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Person {
    /// Stable person identity.
    pub id: PersonId,
    /// Display name.
    pub name: String,
}

impl Person {
    /// Constructs a person with a non-empty display name.
    pub fn new(id: PersonId, name: impl Into<String>) -> Result<Self, CoreError> {
        let name = name.into();
        validate_text("person.name", &name, 512)?;
        Ok(Self { id, name })
    }
}

/// Stable credit roles shared across supported media.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CreditRole {
    /// Director or episode director.
    Director,
    /// Screenwriter or script writer.
    Writer,
    /// Performer portraying a character.
    Actor,
    /// Book author.
    Author,
    /// Book editor.
    Editor,
    /// Translator.
    Translator,
    /// Music performer.
    Performer,
    /// Composer.
    Composer,
    /// Producer.
    Producer,
}

/// One person-role association.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Credit {
    /// Credited person.
    pub person: Person,
    /// Their role.
    pub role: CreditRole,
    /// Optional character or role detail.
    pub character: Option<String>,
}

impl Credit {
    /// Constructs a credit without role detail.
    pub const fn new(person: Person, role: CreditRole) -> Self {
        Self {
            person,
            role,
            character: None,
        }
    }

    /// Adds a character or role detail.
    pub fn with_character(mut self, character: impl Into<String>) -> Result<Self, CoreError> {
        let character = character.into();
        validate_text("credit.character", &character, 512)?;
        self.character = Some(character);
        Ok(self)
    }
}

/// A normalized genre identifier.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Genre(String);

impl Genre {
    /// Constructs a genre.
    pub fn new(value: impl Into<String>) -> Result<Self, CoreError> {
        let value = value.into();
        validate_text("genre", &value, 128)?;
        Ok(Self(value))
    }

    /// Returns the genre text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Artwork purpose.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtworkKind {
    Poster,
    Backdrop,
    Banner,
    Logo,
    Cover,
    Profile,
}

/// A reference to provider-hosted or local artwork.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtworkReference {
    /// Artwork purpose.
    pub kind: ArtworkKind,
    /// Provider URL or local path, retained as data without access.
    pub location: String,
    /// Stable provider artwork identity, when available.
    pub external_id: Option<ExternalId>,
}

impl ArtworkReference {
    /// Constructs an artwork reference.
    pub fn new(kind: ArtworkKind, location: impl Into<String>) -> Result<Self, CoreError> {
        let location = location.into();
        validate_text("artwork.location", &location, 4096)?;
        Ok(Self {
            kind,
            location,
            external_id: None,
        })
    }

    /// Adds a stable provider artwork identity.
    pub fn with_external_id(mut self, external_id: ExternalId) -> Self {
        self.external_id = Some(external_id);
        self
    }
}

/// A rating kept separate by rating system.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Rating {
    /// Rating system or provider namespace.
    pub system: String,
    /// Observed score.
    pub value: f32,
    /// Maximum possible score.
    pub maximum: f32,
}

impl Rating {
    /// Constructs a finite bounded rating.
    pub fn new(system: impl Into<String>, value: f32, maximum: f32) -> Result<Self, CoreError> {
        let system = system.into();
        validate_text("rating.system", &system, 128)?;
        if !value.is_finite()
            || !maximum.is_finite()
            || maximum <= 0.0
            || value < 0.0
            || value > maximum
        {
            return Err(CoreError::InvalidDomainValue {
                field: "rating.value",
                value: format!("{value}/{maximum}"),
            });
        }
        Ok(Self {
            system,
            value,
            maximum,
        })
    }
}

/// A content classification retained with its rating system.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContentRating {
    /// Classification system or territory.
    pub system: String,
    /// Classification value.
    pub value: String,
}

impl ContentRating {
    /// Constructs a content rating.
    pub fn new(system: impl Into<String>, value: impl Into<String>) -> Result<Self, CoreError> {
        let system = system.into();
        let value = value.into();
        validate_text("content_rating.system", &system, 128)?;
        validate_text("content_rating.value", &value, 128)?;
        Ok(Self { system, value })
    }
}

/// Runtime or track duration in whole seconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Duration(u64);

impl Duration {
    /// Constructs a duration from seconds.
    pub const fn from_seconds(seconds: u64) -> Self {
        Self(seconds)
    }
    /// Returns whole seconds.
    pub const fn as_seconds(self) -> u64 {
        self.0
    }
}

/// A source path supplied by an application; no I/O is performed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SourcePath(String);

impl SourcePath {
    /// Constructs a non-empty source path fact.
    pub fn new(value: impl Into<String>) -> Result<Self, CoreError> {
        let value = value.into();
        validate_path("source_path", &value)?;
        Ok(Self(value))
    }
    /// Returns the original path string.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A typed local asset path fact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AssetPath(String);

impl AssetPath {
    /// Constructs a non-empty asset path fact.
    pub fn new(value: impl Into<String>) -> Result<Self, CoreError> {
        let value = value.into();
        validate_path("asset_path", &value)?;
        Ok(Self(value))
    }
    /// Returns the original path string.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Known local asset categories.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssetKind {
    Video,
    Audio,
    BookFile,
    Subtitle,
    Artwork,
    Sidecar,
}

/// Optional facts observed about a local file.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalFileFacts {
    /// File size in bytes when observed.
    pub size_bytes: Option<u64>,
    /// Lowercase filename extension when known.
    pub extension: Option<String>,
    /// MIME type when known.
    pub media_type: Option<String>,
}

/// A media asset modeled without filesystem access.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Asset {
    /// Stable asset identity.
    pub id: AssetId,
    /// Caller-supplied source path.
    pub source_path: SourcePath,
    /// Asset category.
    pub kind: AssetKind,
    /// Optional local-file observations.
    pub facts: LocalFileFacts,
}

impl Asset {
    /// Constructs an asset without probing the path.
    pub const fn new(id: AssetId, source_path: SourcePath, kind: AssetKind) -> Self {
        Self {
            id,
            source_path,
            kind,
            facts: LocalFileFacts {
                size_bytes: None,
                extension: None,
                media_type: None,
            },
        }
    }
}

pub(super) fn validate_text(
    field: &'static str,
    value: &str,
    maximum: usize,
) -> Result<(), CoreError> {
    if value.is_empty() || value.len() > maximum || value.chars().any(char::is_control) {
        Err(CoreError::InvalidDomainValue {
            field,
            value: value.to_owned(),
        })
    } else {
        Ok(())
    }
}

fn validate_path(field: &'static str, value: &str) -> Result<(), CoreError> {
    if value.is_empty() || value.len() > 16_384 || value.contains('\0') {
        Err(CoreError::InvalidDomainValue {
            field,
            value: value.to_owned(),
        })
    } else {
        Ok(())
    }
}
