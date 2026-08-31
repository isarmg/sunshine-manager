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
SUNSHINE_MANAGER_STATIC_DIR=/opt/isarmg/sunshine-manager/releases/0.7.0/web
SUNSHINE_MANAGER_SESSION_TTL_SECONDS=43200
SUNSHINE_MANAGER_SESSION_IDLE_TTL_SECONDS=1800
SUNSHINE_MANAGER_SESSION_COOKIE_SECURE=true
SUNSHINE_MANAGER_BOOTSTRAP_ADMIN_EMAIL=admin@example.com
SUNSHINE_MANAGER_BOOTSTRAP_ADMIN_PASSWORD=<initial admin password>
SUNSHINE_MANAGER_COVER_URL_ALLOWLIST=covers.example.com,cdn.example.com
SUNSHINE_MANAGER_COVER_PROXY_ORIGIN=https://sunshine-manager.internal:18104/
```

Then:

```bash
sunshine-manager serve
```

Plain `serve` is only for an unbound development build. A source-bound release binary rejects it;
the version-fixed systemd unit uses `serve-release --root .../releases/0.7.0`, which verifies the
entire immutable tree and its Web binding before opening the database or listening on the network.

`sunshine-manager identity` prints the binary's strict machine-readable release contract.
Installers and deployment checks must match every field, including the application version,
API prefix, schema fingerprint, compilation target and full source revision; no earlier or
unbound development identity is accepted as a release.

## Immutable release archive

Create the annotated `v0.7.0` tag on a completely clean source commit, then build into an existing
absolute output directory outside this repository:

```bash
python3 scripts/package-release.py /absolute/release-output
```

The packager refuses a lightweight, missing or differently targeted tag and never replaces an
existing archive or checksum. It builds the real Web application and optimized Rust binary, binds
the binary to the full source commit, writes a strict file/mode/size/SHA-256 manifest, verifies the
staged and re-extracted trees, starts the relocated service, fetches its compiled asset, and proves
that a tampered asset is rejected. The archive contains only `0.7.0/`; its included `README.md`
documents the exact installation layout and verification command.

`SUNSHINE_MANAGER_STATIC_DIR` is mandatory and must resolve to the current release's complete
asset directory containing `index.html`. Production rejects links, special files, hard-linked
assets, service-owned assets and group/world-writable content; install releases as root-owned,
read-only trees. The systemd unit points this setting directly at `releases/0.7.0/web` and never
relies on a mutable `current` link or process working directory.

The 0.7 API accepts only `POST /api/v2/auth/login`, `POST /api/v2/auth/logout` and
`GET /api/v2/auth/session`. Sunshine endpoints are available only under
`/api/v2/sunshine/*`; older and unversioned paths are not aliases. Browser sessions use
random tokens whose SHA-256 digests are
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
`GET /api/v2/sunshine/operations/{operation_id}`. States are `pending`, `running`,
`succeeded`, `failed` and `unknown`. An `unknown` result means the remote side effect may
have happened and must not be retried blindly. Requests are stored only as SecretBox
ciphertext plus a SHA-256 fingerprint; the query never returns the actor, request, upstream
error body or credentials.

The process recovers pending operations after restart and changes interrupted `running`
operations to `unknown`. Work is serialized per Sunshine host while different hosts can run
in parallel. Operation creation and its requested audit event commit together, as do terminal
state changes and completion events. A durable idempotent audit outbox is redelivered after
restart.

Run exactly one active Sunshine Manager process for each SQLite database. The process now holds
an exclusive, non-blocking instance lock beside that database for its full lifetime, so a second
worker fails closed instead of bypassing the process-local per-host execution mutex. A separate
shared maintenance lock lets the independent `isarmg-upgrade` tool take a consistent online
backup. Restore, upgrade and administrator maintenance take the lock exclusively and therefore
require the service to be stopped.
On Linux, the database parent and lock files are opened with `openat2` beneath a retained directory
descriptor. Symbolic-link traversal, special files and hard-linked database or lock aliases are
rejected before SQLite opens the file.

## Current database contract

Version 0.7.0 creates exactly one current schema in a database file that does not yet exist.
Every existing database must contain the exact `sunshine-manager` application version, schema
revision and canonical DDL fingerprint. Missing metadata, an older version, a migration ledger or
schema drift is rejected without modifying the database or its sidecars. There is no migration,
backup or restore implementation in this product.

Use the independent `isarmg-upgrade` repository for version-to-version adapters, consistent
backup, verification and restore. It is an offline operations tool and is not a Sunshine Manager
runtime dependency.

The runtime accepts only `SUNSHINE_MANAGER_CREDENTIAL_KEY_ID` and its current key. Re-encrypting
data for a replacement key is an explicit offline `isarmg-upgrade` operation; Sunshine Manager
does not carry a previous-key compatibility keyring.

`doctor` verifies the product schema, SQLite integrity and foreign keys, proves that the database
accepts a transaction which is then rolled back, and decrypts all stored host credentials and
durable operation requests with the configured credential key. It never contacts a Sunshine host
and does not retain its write probe.

Cover uploads accept HTTPS URLs only when their DNS host is listed exactly in
`SUNSHINE_MANAGER_COVER_URL_ALLOWLIST` and every address currently returned by DNS is public.
Immediately before execution, Manager resolves the name once, pins the request to that complete
public address set, disables redirects, limits the body to 8 MiB, and accepts only the supported
image media types. Sunshine receives only a one-time internal URL bound to the Host ID, Operation
ID, resolved Sunshine source address, and a 30-second lifetime; it never receives or resolves the
original external URL. `SUNSHINE_MANAGER_COVER_PROXY_ORIGIN` is therefore required whenever the
allowlist is non-empty. It must be an HTTPS origin reachable directly from managed Sunshine hosts;
do not route this one-time path through a public reverse proxy because authorization uses the
transport peer address and ignores forwarded headers.
