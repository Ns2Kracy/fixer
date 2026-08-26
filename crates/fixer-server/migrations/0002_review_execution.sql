ALTER TABLE jobs ADD COLUMN review_decision_json TEXT
    CHECK (review_decision_json IS NULL OR json_valid(review_decision_json));

CREATE TABLE job_executions (
    job_id INTEGER PRIMARY KEY REFERENCES jobs(id) ON DELETE CASCADE,
    idempotency_key TEXT NOT NULL CHECK (
        length(idempotency_key) BETWEEN 1 AND 256
    ),
    request_fingerprint TEXT NOT NULL CHECK (
        length(request_fingerprint) BETWEEN 1 AND 256
    ),
    created_at_ms INTEGER NOT NULL CHECK (created_at_ms >= 0)
);
