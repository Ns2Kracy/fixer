CREATE TABLE jobs (
    id INTEGER PRIMARY KEY AUTOINCREMENT CHECK (id > 0),
    input_json TEXT NOT NULL CHECK (json_valid(input_json)),
    state TEXT NOT NULL CHECK (state IN (
        'queued',
        'scanning',
        'searching',
        'resolving',
        'awaiting_confirmation',
        'planning',
        'writing',
        'completed',
        'failed',
        'cancelled',
        'interrupted'
    )),
    progress_json TEXT CHECK (progress_json IS NULL OR json_valid(progress_json)),
    review_json TEXT CHECK (review_json IS NULL OR json_valid(review_json)),
    plan_json TEXT CHECK (plan_json IS NULL OR json_valid(plan_json)),
    execution_json TEXT CHECK (execution_json IS NULL OR json_valid(execution_json)),
    created_at_ms INTEGER NOT NULL CHECK (created_at_ms >= 0),
    updated_at_ms INTEGER NOT NULL CHECK (updated_at_ms >= created_at_ms)
);

CREATE INDEX jobs_state_id_idx ON jobs (state, id);
