use std::fmt;

use super::token::{SecretError, digest, issue_secret};

pub struct IssuedSession {
    token: String,
    csrf_token: String,
    expires_at_ms: i64,
}

impl IssuedSession {
    pub(crate) fn new(token: String, csrf_token: String, expires_at_ms: i64) -> Self {
        Self {
            token,
            csrf_token,
            expires_at_ms,
        }
    }

    pub fn token(&self) -> &str {
        &self.token
    }

    pub fn csrf_token(&self) -> &str {
        &self.csrf_token
    }

    pub const fn expires_at_ms(&self) -> i64 {
        self.expires_at_ms
    }
}

impl fmt::Debug for IssuedSession {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("IssuedSession")
            .field("token", &"[REDACTED]")
            .field("csrf_token", &"[REDACTED]")
            .field("expires_at_ms", &self.expires_at_ms)
            .finish()
    }
}

pub(crate) struct SessionSecrets {
    pub token: String,
    pub token_digest: [u8; 32],
    pub csrf_token: String,
    pub csrf_digest: [u8; 32],
}

pub(crate) fn issue_session_secrets() -> Result<SessionSecrets, SecretError> {
    let token = issue_secret("fixer_session_")?;
    let csrf_token = issue_secret("fixer_csrf_")?;
    Ok(SessionSecrets {
        token_digest: digest(&token),
        csrf_digest: digest(&csrf_token),
        token,
        csrf_token,
    })
}
