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
SUNSHINE_MANAGER_SESSION_SECRET=<base64 at least 32 bytes>
SUNSHINE_MANAGER_BOOTSTRAP_ADMIN_EMAIL=admin@example.com
SUNSHINE_MANAGER_BOOTSTRAP_ADMIN_PASSWORD=<initial admin password>
```

Then:

```bash
sunshine-manager serve
```

The local API uses `POST /api/v1/auth/login`, `POST /api/v1/auth/logout` and
`GET /api/v1/auth/session`. Business endpoints remain under
`/api/services/sunshine/*`.
