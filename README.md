# UnionC Sunshine worker

This repository contains the Sunshine business module source used by Union. The
module is a Builder-packaged private process, not an independently supported
product, public HTTP service, crate or binary release. `union-builder` pins an
immutable revision of this repository when composing a Union release. Union
then discovers, validates and supervises the bundled process without linking
its business code into Core or the Web Shell; Union remains the only public
gateway.

The repository-root package contract is described by `manifest.json`, `permissions.json`,
`config/schema.json`, `version.json`, `frontend/` and the module-owned `migrations/`. Builder
decides whether this package is present in an immutable Union release; at runtime Union may enable
or stop only packages already present in that release and never downloads replacement business
code.

## Boundary

- Default and documented endpoint: `127.0.0.1:18104`.
- Any configured non-loopback bind is rejected.
- Every request, including `/health/live` and `/health/ready`, requires the
  shared `gateway-v1` four-header proof: protocol, audience `sunshine`, the
  process-scoped 64-hex token and prefix `/api/modules/sunshine`.
- Health responses echo the protocol and audience headers so Union does not
  open its proxy until it has proved the exact worker contract.
- Browser `Cookie` headers are always rejected. Union consumes its own session,
  signs the internal request and strips browser credentials before forwarding.
- The worker owns its PostgreSQL database and `sunshine` schema, its migration history and its
  encrypted Sunshine upstream passwords. It cannot read the Core database, sessions, another
  module's database or `AppState`.

The worker preserves its internal route contract under `/api/services/sunshine/hosts`; Manifest
v1 maps canonical `/api/modules/sunshine/*` routes without a module-specific Core branch.

## Supervisor-supplied worker environment

```text
SUNSHINE_DATABASE_URL=postgresql://sunshine_runtime:...@127.0.0.1/sunshine
UNION_MODULE_PROTOCOL=gateway-v1
UNION_MODULE_AUDIENCE=sunshine
UNION_MODULE_TOKEN=<64 lowercase hexadecimal characters; supplied by Union>
UNION_MODULE_PREFIX=/api/modules/sunshine
SUNSHINE_CREDENTIAL_KEY=<base64 32 bytes; module-owned encryption key>
SUNSHINE_CREDENTIAL_KEY_ID=primary
SUNSHINE_BIND=127.0.0.1:18104
UNION_PLUGIN_BIND=127.0.0.1:18104
SUNSHINE_PRODUCTION=true
```

Operators do not launch this binary with that block. They store values through the module's
`config/schema.json`; Runtime maps only Manifest-allowlisted fields, supplies the dynamic bind and
creates the gateway identity. The expanded names above document the private process contract.

The PostgreSQL administrator must create schema `sunshine` owned by the module
role. Startup applies only migrations from `migrations/`; it deliberately does
not create roles, databases or schemas.

## Legacy data cutover

See [docs/sqlite-cutover.md](docs/sqlite-cutover.md). The importer decrypts the
legacy row with the supplied Union key, immediately re-encrypts it with the
Sunshine key, and stores no plaintext migration copy. Exact before/after module
ciphertext is retained as a rollback journal.

## Development

The repository has an independent Cargo package and lockfile so its contract can
be validated without checking out Union Core. Running the worker directly is
only a developer test; deployed instances always obtain their gateway identity
and lifecycle from Union:

```bash
cargo fmt --all -- --check
cargo check --locked --all-targets
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
node --test frontend/entry.test.mjs
```

This source repository may be versioned and built independently for composition
purposes, but only the complete Union distribution is an operator-facing release.
