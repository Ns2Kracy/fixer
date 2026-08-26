use std::fmt;

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use sha2::{Digest, Sha256};
use thiserror::Error;

pub(crate) const DIGEST_BYTES: usize = 32;
const SECRET_BYTES: usize = 32;

/// A newly issued API token. The plaintext is only available on this value.
pub struct IssuedApiToken {
    id: i64,
    token: String,
}

impl IssuedApiToken {
    pub(crate) fn new(id: i64, token: String) -> Self {
        Self { id, token }
    }

    pub const fn id(&self) -> i64 {
        self.id
    }

    pub fn token(&self) -> &str {
        &self.token
    }
}

impl fmt::Debug for IssuedApiToken {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("IssuedApiToken")
            .field("id", &self.id)
            .field("token", &"[REDACTED]")
            .finish()
    }
}

#[derive(Debug, Error)]
pub enum SecretError {
    #[error("operating-system randomness is unavailable")]
    Randomness,
}

pub(crate) fn issue_secret(prefix: &str) -> Result<String, SecretError> {
    let mut bytes = [0_u8; SECRET_BYTES];
    getrandom::fill(&mut bytes).map_err(|_| SecretError::Randomness)?;
    Ok(format!("{prefix}{}", URL_SAFE_NO_PAD.encode(bytes)))
}

pub(crate) fn digest(secret: &str) -> [u8; DIGEST_BYTES] {
    Sha256::digest(secret.as_bytes()).into()
}
