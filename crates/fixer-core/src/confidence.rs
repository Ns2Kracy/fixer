//! Confidence values constrained to the inclusive unit interval.

use crate::CoreError;
use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error as _};

/// A finite confidence value in `0.0..=1.0`.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct Confidence(f32);

impl Confidence {
    /// Constructs a validated confidence value.
    pub fn new(value: f32) -> Result<Self, CoreError> {
        if value.is_finite() && (0.0..=1.0).contains(&value) {
            Ok(Self(value))
        } else {
            Err(CoreError::InvalidConfidence { value })
        }
    }

    /// Returns the underlying value.
    pub const fn get(self) -> f32 {
        self.0
    }
}

impl Serialize for Confidence {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_f32(self.0)
    }
}

impl<'de> Deserialize<'de> for Confidence {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = f32::deserialize(deserializer)?;
        Self::new(value).map_err(D::Error::custom)
    }
}
