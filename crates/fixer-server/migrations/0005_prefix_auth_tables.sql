ALTER TABLE single_user_auth RENAME TO fixer_users;
ALTER TABLE auth_sessions RENAME TO fixer_sessions;
ALTER TABLE api_tokens RENAME TO fixer_api_tokens;

DROP INDEX auth_sessions_expiry_idx;
CREATE INDEX fixer_sessions_expiry_idx ON fixer_sessions (expires_at_ms);
