# Sunshine Manager

Sunshine Manager is an independent service for managing Sunshine hosts. It keeps the
Sunshine API client and host configuration logic, and adds local administrator accounts,
sessions, configuration and an HTTP API.

## Build

```bash
cargo build --release
```

## Run

The server reads environment variables with the `SUNSHINE_MANAGER_` prefix:

```text
SUNSHINE_MANAGER_DATABASE_URL=sqlite:///var/lib/isarmg/sunshine-manager/db/app.db
SUNSHINE_MANAGER_CREDENTIAL_KEY=<base64 32 bytes>
SUNSHINE_MANAGER_SESSION_TTL_SECONDS=43200
SUNSHINE_MANAGER_SESSION_IDLE_TTL_SECONDS=1800
SUNSHINE_MANAGER_SESSION_COOKIE_SECURE=true
SUNSHINE_MANAGER_BOOTSTRAP_ADMIN_EMAIL=admin@example.com
SUNSHINE_MANAGER_BOOTSTRAP_ADMIN_PASSWORD=<initial admin password>
```

Then:

```bash
sunshine-manager serve
```

The local API uses `POST /api/v1/auth/login`, `POST /api/v1/auth/logout` and
`GET /api/v1/auth/session`. Business endpoints remain under
`/api/services/sunshine/*`. Browser sessions use random tokens whose SHA-256 digests are
stored in the product's SQLite database. Mutating requests require the session-bound
`X-CSRF-Token` returned by login or the session endpoint and a matching `Origin`/`Host`.
Login requests have bounded bodies, per-source and per-account budgets, a bounded global
Argon2 concurrency gate, and the same password-hash work for unknown and known users.
