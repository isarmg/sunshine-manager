CREATE TABLE auth_users (
    user_id           TEXT PRIMARY KEY,
    email             TEXT NOT NULL UNIQUE
                           CHECK (email = lower(trim(email)) AND length(trim(email)) BETWEEN 3 AND 255),
    password_hash     TEXT NOT NULL CHECK (length(password_hash) > 0),
    active            INTEGER NOT NULL DEFAULT 1 CHECK (active IN (0, 1)),
    created_at_micros INTEGER NOT NULL
);
