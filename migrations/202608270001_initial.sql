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
    verify_tls         INTEGER NOT NULL CHECK (verify_tls IN (0, 1)),
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
    created_at_micros  INTEGER NOT NULL
);

CREATE INDEX audit_logs_created_at_idx ON audit_logs(created_at_micros DESC);
