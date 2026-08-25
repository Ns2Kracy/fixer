//! Localized values and deterministic locale selection.

use crate::CoreError;
use language_tags::LanguageTag as ParsedLanguageTag;
use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error as _};
use std::{
    fmt,
    hash::{Hash, Hasher},
    str::FromStr,
};

/// A validated BCP 47 language tag that preserves the caller's spelling.
#[derive(Clone)]
pub struct LanguageTag {
    original: String,
    normalized: String,
    primary_language: String,
}

impl LanguageTag {
    /// Returns the original validated spelling.
    pub fn as_str(&self) -> &str {
        &self.original
    }

    /// Returns a lowercase canonical form used for lookup and equality.
    pub fn normalized(&self) -> &str {
        &self.normalized
    }

    /// Returns the primary language subtag.
    pub fn primary_language(&self) -> &str {
        &self.primary_language
    }

    fn parent_normalized_tags(&self) -> impl Iterator<Item = String> + '_ {
        let mut parent = self.normalized.as_str();
        std::iter::from_fn(move || {
            let (next, _) = parent.rsplit_once('-')?;
            parent = next;
            Some(next.to_owned())
        })
    }
}

impl FromStr for LanguageTag {
    type Err = CoreError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let parsed =
            ParsedLanguageTag::parse(input).map_err(|error| CoreError::InvalidLanguageTag {
                input: input.to_owned(),
                reason: format!("{error:?}"),
            })?;
        parsed
            .validate()
            .map_err(|error| CoreError::InvalidLanguageTag {
                input: input.to_owned(),
                reason: format!("{error:?}"),
            })?;
        let canonical = parsed.canonicalize().unwrap_or_else(|_| parsed.clone());
        Ok(Self {
            original: input.to_owned(),
            normalized: canonical.as_str().to_ascii_lowercase(),
            primary_language: canonical.primary_language().to_ascii_lowercase(),
        })
    }
}

impl PartialEq for LanguageTag {
    fn eq(&self, other: &Self) -> bool {
        self.normalized == other.normalized
    }
}

impl Eq for LanguageTag {}

impl PartialOrd for LanguageTag {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for LanguageTag {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.normalized.cmp(&other.normalized)
    }
}

impl Hash for LanguageTag {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.normalized.hash(state);
    }
}

impl fmt::Debug for LanguageTag {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("LanguageTag")
            .field(&self.original)
            .finish()
    }
}

impl fmt::Display for LanguageTag {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.original)
    }
}

impl Serialize for LanguageTag {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.original)
    }
}

impl<'de> Deserialize<'de> for LanguageTag {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        String::deserialize(deserializer)?
            .parse()
            .map_err(D::Error::custom)
    }
}

/// One tagged or untagged localized value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LocalizedEntry<T> {
    /// A value associated with a BCP 47 tag.
    Tagged { language: LanguageTag, value: T },
    /// A value whose source did not declare a language.
    Untagged { value: T },
}

/// All tagged and untagged alternatives for one metadata field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct LocalizedValue<T> {
    entries: Vec<LocalizedEntry<T>>,
}

impl<T> Default for LocalizedValue<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> LocalizedValue<T> {
    /// Creates an empty localized value set.
    pub const fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// Adds a tagged value while preserving existing alternatives.
    pub fn insert(&mut self, language: impl AsRef<str>, value: T) -> Result<(), CoreError> {
        self.entries.push(LocalizedEntry::Tagged {
            language: language.as_ref().parse()?,
            value,
        });
        Ok(())
    }

    /// Adds a value without a declared language.
    pub fn insert_untagged(&mut self, value: T) {
        self.entries.push(LocalizedEntry::Untagged { value });
    }

    /// Returns every value in insertion order.
    pub fn entries(&self) -> &[LocalizedEntry<T>] {
        &self.entries
    }

    /// Selects one value according to an ordered locale policy.
    pub fn select(&self, policy: &LocalePolicy) -> Option<&T> {
        for preferred in &policy.preferred {
            if let Some(value) = self.exact(preferred.normalized()) {
                return Some(value);
            }
        }
        if policy.parent_language_fallback {
            for preferred in &policy.preferred {
                for parent in preferred.parent_normalized_tags() {
                    if let Some(value) = self.exact(&parent) {
                        return Some(value);
                    }
                }
            }
        }
        if policy.undefined_language_fallback {
            if let Some(value) = self.exact("und") {
                return Some(value);
            }
        }
        if policy.untagged_fallback {
            return self.entries.iter().find_map(|entry| match entry {
                LocalizedEntry::Untagged { value } => Some(value),
                LocalizedEntry::Tagged { .. } => None,
            });
        }
        None
    }

    fn exact(&self, normalized: &str) -> Option<&T> {
        self.entries.iter().find_map(|entry| match entry {
            LocalizedEntry::Tagged { language, value } if language.normalized() == normalized => {
                Some(value)
            }
            _ => None,
        })
    }
}

/// Ordered preferred tags and explicit fallback behavior.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalePolicy {
    preferred: Vec<LanguageTag>,
    parent_language_fallback: bool,
    undefined_language_fallback: bool,
    untagged_fallback: bool,
}

impl LocalePolicy {
    /// Builds a policy with parent-language, `und`, and untagged fallback enabled.
    pub fn new<I, S>(preferred: I) -> Result<Self, CoreError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let preferred = preferred
            .into_iter()
            .map(|tag| tag.as_ref().parse())
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            preferred,
            parent_language_fallback: true,
            undefined_language_fallback: true,
            untagged_fallback: true,
        })
    }

    /// Returns the ordered preferred language tags.
    pub fn preferred(&self) -> &[LanguageTag] {
        &self.preferred
    }

    /// Enables or disables parent-language fallback.
    pub const fn with_parent_language_fallback(mut self, enabled: bool) -> Self {
        self.parent_language_fallback = enabled;
        self
    }

    /// Enables or disables `und` fallback.
    pub const fn with_undefined_language_fallback(mut self, enabled: bool) -> Self {
        self.undefined_language_fallback = enabled;
        self
    }

    /// Enables or disables untagged fallback.
    pub const fn with_untagged_fallback(mut self, enabled: bool) -> Self {
        self.untagged_fallback = enabled;
        self
    }
}
