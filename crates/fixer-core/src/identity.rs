//! Stable provider and external identifiers.

use crate::CoreError;
use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error as _};
use std::fmt;

/// A validated, stable identifier for a metadata provider.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ProviderId(String);

impl ProviderId {
    /// Constructs a provider identifier.
    ///
    /// IDs use lowercase ASCII letters, digits, dots, dashes, and underscores.
    pub fn new(input: impl Into<String>) -> Result<Self, CoreError> {
        let input = input.into();
        let mut chars = input.chars();
        let first_is_valid = chars
            .next()
            .is_some_and(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit());
        let rest_is_valid = chars.all(|ch| {
            ch.is_ascii_lowercase() || ch.is_ascii_digit() || matches!(ch, '.' | '-' | '_')
        });
        if input.len() <= 128 && first_is_valid && rest_is_valid {
            Ok(Self(input))
        } else {
            Err(CoreError::InvalidProviderId { input })
        }
    }

    /// Returns the identifier as text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for ProviderId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_tuple("ProviderId").field(&self.0).finish()
    }
}

impl fmt::Display for ProviderId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Serialize for ProviderId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for ProviderId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Self::new(String::deserialize(deserializer)?).map_err(D::Error::custom)
    }
}

/// An identifier assigned by an external namespace.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct ExternalId {
    /// Stable namespace such as `tmdb`, `imdb`, or `isbn-13`.
    pub namespace: String,
    /// Identifier value within the namespace.
    pub value: String,
}

impl ExternalId {
    /// Constructs a validated external identifier.
    pub fn new(namespace: impl Into<String>, value: impl Into<String>) -> Result<Self, CoreError> {
        let namespace = namespace.into();
        let value = value.into();
        let namespace_valid = namespace.chars().enumerate().all(|(index, ch)| {
            (index > 0 && matches!(ch, '.' | '-' | '_'))
                || ch.is_ascii_lowercase()
                || ch.is_ascii_digit()
        }) && namespace.len() <= 128
            && !namespace.is_empty();
        if !namespace_valid {
            return Err(CoreError::InvalidExternalId {
                field: "namespace",
                input: namespace,
            });
        }
        if value.is_empty() || value.len() > 512 || value.chars().any(char::is_control) {
            return Err(CoreError::InvalidExternalId {
                field: "value",
                input: value,
            });
        }
        Ok(Self { namespace, value })
    }
}

impl<'de> Deserialize<'de> for ExternalId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct ExternalIdDto {
            namespace: String,
            value: String,
        }

        let dto = ExternalIdDto::deserialize(deserializer)?;
        Self::new(dto.namespace, dto.value).map_err(D::Error::custom)
    }
}
