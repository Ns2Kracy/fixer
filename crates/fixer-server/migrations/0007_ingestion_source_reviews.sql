ALTER TABLE ingestion_sources ADD COLUMN review_kinds_json TEXT
CHECK (review_kinds_json IS NULL OR json_valid(review_kinds_json));
