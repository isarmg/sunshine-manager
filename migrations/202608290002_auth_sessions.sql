ALTER TABLE auth_users
    ADD COLUMN session_version INTEGER NOT NULL DEFAULT 1
        CHECK (session_version > 0);

CREATE TABLE auth_sessions (
    session_id             TEXT PRIMARY KEY,
    user_id                TEXT NOT NULL
                                REFERENCES auth_users(user_id) ON DELETE CASCADE,
    token_hash             BLOB NOT NULL UNIQUE CHECK (length(token_hash) = 32),
    csrf_hash              BLOB NOT NULL CHECK (length(csrf_hash) = 32),
    user_session_version   INTEGER NOT NULL CHECK (user_session_version > 0),
    created_at_micros      INTEGER NOT NULL,
    last_seen_at_micros    INTEGER NOT NULL,
    idle_expires_at_micros INTEGER NOT NULL,
    absolute_expires_at_micros INTEGER NOT NULL,
    revoked_at_micros      INTEGER,
    CHECK (last_seen_at_micros >= created_at_micros),
    CHECK (idle_expires_at_micros > created_at_micros),
    CHECK (absolute_expires_at_micros >= idle_expires_at_micros)
);

CREATE INDEX auth_sessions_user_idx
    ON auth_sessions(user_id, revoked_at_micros);

CREATE INDEX auth_sessions_expiry_idx
    ON auth_sessions(idle_expires_at_micros, absolute_expires_at_micros)
    WHERE revoked_at_micros IS NULL;
