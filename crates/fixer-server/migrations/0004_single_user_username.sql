ALTER TABLE single_user_auth
ADD COLUMN username TEXT
CHECK (username IS NULL OR length(trim(username)) BETWEEN 3 AND 64);
