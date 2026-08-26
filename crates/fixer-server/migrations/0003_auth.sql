CREATE TABLE single_user_auth (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    password_hash TEXT NOT NULL CHECK (length(password_hash) BETWEEN 32 AND 1024),
    updated_at_ms INTEGER NOT NULL CHECK (updated_at_ms >= 0)
);

CREATE TABLE api_tokens (
    id INTEGER PRIMARY KEY AUTOINCREMENT CHECK (id > 0),
    name TEXT NOT NULL CHECK (length(trim(name)) BETWEEN 1 AND 100),
    token_digest BLOB NOT NULL UNIQUE CHECK (length(token_digest) = 32),
    created_at_ms INTEGER NOT NULL CHECK (created_at_ms >= 0),
    revoked_at_ms INTEGER CHECK (revoked_at_ms IS NULL OR revoked_at_ms >= created_at_ms)
);

CREATE TABLE auth_sessions (
    token_digest BLOB PRIMARY KEY CHECK (length(token_digest) = 32),
    csrf_digest BLOB NOT NULL CHECK (length(csrf_digest) = 32),
    created_at_ms INTEGER NOT NULL CHECK (created_at_ms >= 0),
    expires_at_ms INTEGER NOT NULL CHECK (expires_at_ms > created_at_ms)
);

CREATE INDEX auth_sessions_expiry_idx ON auth_sessions (expires_at_ms);
