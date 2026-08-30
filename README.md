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
SUNSHINE_MANAGER_COVER_URL_ALLOWLIST=covers.example.com,cdn.example.com
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

## Remote operations

Sunshine remote mutations are asynchronous durable operations. Every remote write request
must include exactly one `Idempotency-Key` containing 1-128 ASCII letters, digits, `-`, `_`,
`.` or `:`. The tuple of authenticated actor, host, action and key identifies an operation:
repeating the same request returns the original operation, while reusing the key for a
different request returns `409 Conflict`.

Accepted mutations return `202 Accepted` with a non-secret status document:

```json
{
  "operation_id": "op_...",
  "state": "pending",
  "attempt": 0
}
```

Authenticated callers can query their operation at
`GET /api/services/sunshine/operations/{operation_id}`. States are `pending`, `running`,
`succeeded`, `failed` and `unknown`. An `unknown` result means the remote side effect may
have happened and must not be retried blindly. Requests are stored only as SecretBox
ciphertext plus a SHA-256 fingerprint; the query never returns the actor, request, upstream
error body or credentials.

The process recovers pending operations after restart and changes interrupted `running`
operations to `unknown`. Work is serialized per Sunshine host while different hosts can run
in parallel. Operation creation and its requested audit event commit together, as do terminal
state changes and completion events. A durable idempotent audit outbox is redelivered after
restart.

Run exactly one active Sunshine Manager process for each SQLite database. Operation claiming
is persistent, but the per-host execution mutex is process-local; deployments and upgrades
must stop the old process before starting its replacement.

## Backup and restore

`backup-create` uses SQLite's online backup API, refuses to overwrite an existing output,
and verifies integrity, foreign keys and the product schema before reporting success.
`backup-verify` performs the same read-only checks. Stop the service before `restore`; the
command first validates and reconstructs the backup beside the destination, then atomically
replaces the database and verifies the restored file.

Cover uploads accept HTTPS URLs only when their DNS host is listed exactly in
`SUNSHINE_MANAGER_COVER_URL_ALLOWLIST` and every address currently returned by DNS is public.
The policy is checked before creating a new operation and checked again immediately before
the background worker asks Sunshine to fetch the URL; an idempotent replay is resolved before
performing DNS again.
The managed Sunshine machine resolves and fetches the URL itself, so its network must also enforce
an outbound allowlist that blocks private, link-local and metadata networks and unsafe redirects.
