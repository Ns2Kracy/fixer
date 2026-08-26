use std::fmt;

use argon2::{
    Argon2,
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
};
use thiserror::Error;

const MAX_PASSWORD_BYTES: usize = 1024;

/// An encoded Argon2id PHC value. Debug output never exposes the hash.
#[derive(Clone, PartialEq, Eq)]
pub struct PasswordHashValue(String);

impl PasswordHashValue {
    pub fn parse(value: impl Into<String>) -> Result<Self, PasswordError> {
        let value = value.into();
        let parsed = PasswordHash::new(&value).map_err(|_| PasswordError::InvalidHash)?;
        if parsed.algorithm.as_str() != "argon2id" {
            return Err(PasswordError::InvalidHash);
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for PasswordHashValue {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("PasswordHashValue([REDACTED])")
    }
}

#[derive(Debug, Error)]
pub enum PasswordError {
    #[error("password must contain between 1 and {MAX_PASSWORD_BYTES} bytes")]
    InvalidPasswordLength,
    #[error("password hash is invalid")]
    InvalidHash,
    #[error("password hashing failed")]
    HashingFailed,
}

pub fn hash_password(password: &str) -> Result<PasswordHashValue, PasswordError> {
    validate_password(password)?;
    let mut salt_bytes = [0_u8; 16];
    getrandom::fill(&mut salt_bytes).map_err(|_| PasswordError::HashingFailed)?;
    let salt = SaltString::encode_b64(&salt_bytes).map_err(|_| PasswordError::HashingFailed)?;
    let encoded = Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map_err(|_| PasswordError::HashingFailed)?
        .to_string();
    PasswordHashValue::parse(encoded)
}

pub fn verify_password(password: &str, encoded: &PasswordHashValue) -> Result<bool, PasswordError> {
    validate_password(password)?;
    let parsed = PasswordHash::new(encoded.as_str()).map_err(|_| PasswordError::InvalidHash)?;
    Ok(Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .is_ok())
}

fn validate_password(password: &str) -> Result<(), PasswordError> {
    if password.is_empty() || password.len() > MAX_PASSWORD_BYTES {
        return Err(PasswordError::InvalidPasswordLength);
    }
    Ok(())
}
