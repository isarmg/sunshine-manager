ALTER TABLE audit_logs ADD COLUMN outbox_id TEXT;

CREATE UNIQUE INDEX audit_logs_outbox_id_idx
    ON audit_logs(outbox_id);

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
                                      'unknown'
                                  )),
    attempt                  INTEGER NOT NULL DEFAULT 0 CHECK (attempt >= 0),
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
    CHECK (
        (state = 'pending' AND attempt = 0 AND started_at_micros IS NULL
                           AND completed_at_micros IS NULL AND error_code IS NULL)
        OR
        (state = 'running' AND attempt > 0 AND started_at_micros IS NOT NULL
                           AND completed_at_micros IS NULL AND error_code IS NULL)
        OR
        (state = 'succeeded' AND attempt > 0 AND started_at_micros IS NOT NULL
                             AND completed_at_micros IS NOT NULL AND error_code IS NULL)
        OR
        (state IN ('failed', 'unknown') AND attempt > 0
             AND started_at_micros IS NOT NULL
             AND completed_at_micros IS NOT NULL
             AND error_code IS NOT NULL)
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
                                  CHECK (event_kind IN ('requested', 'completed')),
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

CREATE UNIQUE INDEX audit_outbox_operation_event_idx
    ON audit_outbox(operation_id, event_kind);

CREATE INDEX audit_outbox_pending_idx
    ON audit_outbox(delivered_at_micros, created_at_micros, outbox_id);
