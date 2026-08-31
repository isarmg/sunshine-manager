CREATE TABLE hosts (
    host_id            TEXT PRIMARY KEY
                            CHECK (length(trim(host_id)) BETWEEN 1 AND 255),
    name               TEXT NOT NULL
                            CHECK (length(trim(name)) BETWEEN 1 AND 128),
    address            TEXT NOT NULL
                            CHECK (length(trim(address)) BETWEEN 1 AND 253),
    web_port           INTEGER NOT NULL CHECK (web_port BETWEEN 1 AND 65535),
    username           TEXT NOT NULL
                            CHECK (length(trim(username)) BETWEEN 1 AND 256),
    secret             TEXT,
    position           INTEGER NOT NULL CHECK (position >= 0),
    created_at_micros  INTEGER NOT NULL,
    updated_at_micros  INTEGER NOT NULL,
    CHECK (secret IS NULL OR length(secret) > 0)
);

CREATE INDEX hosts_position_idx
    ON hosts(position, created_at_micros, host_id);

CREATE TABLE audit_logs (
    audit_id           INTEGER PRIMARY KEY AUTOINCREMENT,
    action             TEXT NOT NULL CHECK (length(action) BETWEEN 1 AND 128),
    target             TEXT NOT NULL CHECK (length(target) BETWEEN 1 AND 255),
    detail             TEXT,
    actor              TEXT NOT NULL CHECK (length(actor) BETWEEN 1 AND 128),
    created_at_micros  INTEGER NOT NULL,
    outbox_id          TEXT
);

CREATE INDEX audit_logs_created_at_idx
    ON audit_logs(created_at_micros DESC);

CREATE UNIQUE INDEX audit_logs_outbox_id_idx
    ON audit_logs(outbox_id);

CREATE TABLE auth_users (
    user_id            TEXT PRIMARY KEY,
    email              TEXT NOT NULL UNIQUE
                            CHECK (
                                email = lower(trim(email))
                                AND length(trim(email)) BETWEEN 3 AND 255
                            ),
    password_hash      TEXT NOT NULL CHECK (length(password_hash) > 0),
    active             INTEGER NOT NULL DEFAULT 1 CHECK (active IN (0, 1)),
    created_at_micros  INTEGER NOT NULL,
    session_version    INTEGER NOT NULL DEFAULT 1
                            CHECK (session_version > 0)
);

CREATE TABLE auth_sessions (
    session_id                 TEXT PRIMARY KEY,
    user_id                    TEXT NOT NULL
                                     REFERENCES auth_users(user_id) ON DELETE CASCADE,
    token_hash                 BLOB NOT NULL UNIQUE CHECK (length(token_hash) = 32),
    csrf_hash                  BLOB NOT NULL CHECK (length(csrf_hash) = 32),
    user_session_version       INTEGER NOT NULL CHECK (user_session_version > 0),
    created_at_micros          INTEGER NOT NULL,
    last_seen_at_micros        INTEGER NOT NULL,
    idle_expires_at_micros     INTEGER NOT NULL,
    absolute_expires_at_micros INTEGER NOT NULL,
    revoked_at_micros          INTEGER,
    CHECK (last_seen_at_micros >= created_at_micros),
    CHECK (idle_expires_at_micros > created_at_micros),
    CHECK (absolute_expires_at_micros >= idle_expires_at_micros)
);

CREATE INDEX auth_sessions_user_idx
    ON auth_sessions(user_id, revoked_at_micros);

CREATE INDEX auth_sessions_expiry_idx
    ON auth_sessions(idle_expires_at_micros, absolute_expires_at_micros)
    WHERE revoked_at_micros IS NULL;

CREATE TABLE operations (
    operation_id             TEXT PRIMARY KEY
                                  CHECK (length(operation_id) BETWEEN 1 AND 64),
    actor                    TEXT NOT NULL
                                  CHECK (length(actor) BETWEEN 1 AND 128),
    host_id                  TEXT NOT NULL
                                  CHECK (length(host_id) BETWEEN 1 AND 255),
    action                   TEXT NOT NULL
                                  CHECK (length(action) BETWEEN 1 AND 128),
    idempotency_key_hash     BLOB NOT NULL
                                  CHECK (length(idempotency_key_hash) = 32),
    request_fingerprint      BLOB NOT NULL
                                  CHECK (length(request_fingerprint) = 32),
    request_ciphertext       TEXT NOT NULL
                                  CHECK (
                                      substr(request_ciphertext, 1, 12) = 'sunshine:v1:'
                                  ),
    state                    TEXT NOT NULL
                                  CHECK (state IN (
                                      'pending',
                                      'running',
                                      'succeeded',
                                      'failed',
                                      'unknown',
                                      'dead_letter',
                                      'resolved'
                                  )),
    attempt                  INTEGER NOT NULL DEFAULT 0 CHECK (attempt >= 0),
    max_attempts             INTEGER NOT NULL DEFAULT 3
                                  CHECK (max_attempts BETWEEN 1 AND 16),
    created_at_micros        INTEGER NOT NULL,
    updated_at_micros        INTEGER NOT NULL,
    started_at_micros        INTEGER,
    completed_at_micros      INTEGER,
    error_code               TEXT CHECK (
                                  error_code IS NULL OR
                                  (
                                      length(error_code) BETWEEN 1 AND 64
                                      AND error_code NOT GLOB '*[^a-z0-9_]*'
                                  )
                              ),
    dead_letter_at_micros    INTEGER,
    resolved_at_micros       INTEGER,
    resolved_by              TEXT CHECK (
                                  resolved_by IS NULL OR
                                  length(resolved_by) BETWEEN 1 AND 128
                              ),
    resolution               TEXT CHECK (
                                  resolution IS NULL OR
                                  resolution IN ('confirmed_succeeded', 'confirmed_failed')
                              ),
    CHECK (
        (state = 'pending' AND attempt < max_attempts
                           AND started_at_micros IS NULL
                           AND completed_at_micros IS NULL AND error_code IS NULL
                           AND dead_letter_at_micros IS NULL
                           AND resolved_at_micros IS NULL AND resolved_by IS NULL
                           AND resolution IS NULL)
        OR
        (state = 'running' AND attempt > 0 AND attempt <= max_attempts
                           AND started_at_micros IS NOT NULL
                           AND completed_at_micros IS NULL AND error_code IS NULL)
        OR
        (state = 'succeeded' AND attempt > 0 AND started_at_micros IS NOT NULL
                             AND completed_at_micros IS NOT NULL AND error_code IS NULL)
        OR
        (state IN ('failed', 'unknown') AND attempt > 0
             AND started_at_micros IS NOT NULL
             AND completed_at_micros IS NOT NULL
             AND error_code IS NOT NULL)
        OR
        (state = 'dead_letter' AND attempt >= max_attempts
             AND started_at_micros IS NOT NULL
             AND completed_at_micros IS NOT NULL
             AND error_code IS NOT NULL
             AND dead_letter_at_micros IS NOT NULL)
        OR
        (state = 'resolved' AND attempt > 0
             AND completed_at_micros IS NOT NULL
             AND resolved_at_micros IS NOT NULL
             AND resolved_by IS NOT NULL
             AND resolution IS NOT NULL)
    )
);

CREATE UNIQUE INDEX operations_idempotency_idx
    ON operations(actor, host_id, action, idempotency_key_hash);

CREATE INDEX operations_pending_idx
    ON operations(state, created_at_micros, operation_id);

CREATE INDEX operations_host_idx
    ON operations(host_id, created_at_micros DESC, operation_id DESC);

CREATE TABLE audit_outbox (
    outbox_id                TEXT PRIMARY KEY
                                  CHECK (length(outbox_id) BETWEEN 1 AND 64),
    operation_id             TEXT NOT NULL REFERENCES operations(operation_id)
                                  ON DELETE CASCADE,
    event_kind               TEXT NOT NULL
                                  CHECK (event_kind IN ('requested', 'completed', 'resolved')),
    action                   TEXT NOT NULL
                                  CHECK (length(action) BETWEEN 1 AND 128),
    target                   TEXT NOT NULL
                                  CHECK (length(target) BETWEEN 1 AND 255),
    actor                    TEXT NOT NULL
                                  CHECK (length(actor) BETWEEN 1 AND 128),
    detail                   TEXT NOT NULL
                                  CHECK (length(detail) BETWEEN 1 AND 512),
    created_at_micros        INTEGER NOT NULL,
    delivered_at_micros      INTEGER,
    delivery_attempt         INTEGER NOT NULL DEFAULT 0
                                  CHECK (delivery_attempt >= 0)
);

CREATE INDEX audit_outbox_pending_idx
    ON audit_outbox(delivered_at_micros, created_at_micros, outbox_id);
