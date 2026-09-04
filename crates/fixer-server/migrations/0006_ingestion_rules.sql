CREATE TABLE ingestion_rules (
    id INTEGER PRIMARY KEY AUTOINCREMENT CHECK (id > 0),
    name TEXT NOT NULL CHECK (length(trim(name)) BETWEEN 1 AND 100),
    source_root_id TEXT NOT NULL CHECK (length(source_root_id) BETWEEN 1 AND 128),
    source_relative_path TEXT NOT NULL CHECK (length(source_relative_path) <= 4096),
    destination_root_id TEXT NOT NULL CHECK (length(destination_root_id) BETWEEN 1 AND 128),
    destination_relative_path TEXT NOT NULL CHECK (length(destination_relative_path) <= 4096),
    media_kind_mode TEXT NOT NULL CHECK (media_kind_mode IN ('auto', 'fixed')),
    fixed_media_kind TEXT CHECK (fixed_media_kind IN (
        'anime', 'book', 'movie', 'music', 'television'
    )),
    placement TEXT NOT NULL CHECK (placement IN (
        'move', 'copy', 'hardlink', 'symlink', 'reflink'
    )),
    path_template_override TEXT CHECK (
        path_template_override IS NULL OR length(path_template_override) BETWEEN 1 AND 4096
    ),
    enabled INTEGER NOT NULL CHECK (enabled IN (0, 1)),
    last_error TEXT CHECK (last_error IS NULL OR length(last_error) BETWEEN 1 AND 1000),
    created_at_ms INTEGER NOT NULL CHECK (created_at_ms >= 0),
    updated_at_ms INTEGER NOT NULL CHECK (updated_at_ms >= created_at_ms),
    CHECK (
        (media_kind_mode = 'auto' AND fixed_media_kind IS NULL) OR
        (media_kind_mode = 'fixed' AND fixed_media_kind IS NOT NULL)
    )
);

CREATE INDEX ingestion_rules_enabled_id_idx ON ingestion_rules (enabled, id);

CREATE TABLE ingestion_sources (
    id INTEGER PRIMARY KEY AUTOINCREMENT CHECK (id > 0),
    rule_id INTEGER NOT NULL REFERENCES ingestion_rules(id) ON DELETE CASCADE,
    relative_source_path TEXT NOT NULL CHECK (
        length(relative_source_path) BETWEEN 1 AND 4096
    ),
    size_bytes INTEGER NOT NULL CHECK (size_bytes >= 0),
    modified_at_ms INTEGER NOT NULL CHECK (modified_at_ms >= 0),
    status TEXT NOT NULL CHECK (status IN (
        'watching', 'processing', 'needs_review', 'paused', 'error'
    )),
    job_id INTEGER REFERENCES jobs(id) ON DELETE SET NULL,
    created_at_ms INTEGER NOT NULL CHECK (created_at_ms >= 0),
    updated_at_ms INTEGER NOT NULL CHECK (updated_at_ms >= created_at_ms),
    UNIQUE (rule_id, relative_source_path, size_bytes, modified_at_ms),
    UNIQUE (job_id)
);

CREATE INDEX ingestion_sources_rule_id_idx ON ingestion_sources (rule_id, id);
