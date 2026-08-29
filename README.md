# UnionC Sunshine worker

This repository contains the Sunshine business module source used by Union. The
module is a Builder-packaged private process, not an independently supported
product, public HTTP service, crate or binary release. `union-builder` pins an
immutable revision of this repository when composing a Union release. Union
then discovers, validates and supervises the bundled process without linking
its business code into Core or the Web Shell; Union remains the only public
gateway.

The repository-root package contract is described by `manifest.json`, `permissions.json`,
`config/schema.json`, `version.json`, `frontend/` and the module-owned `migrations/`. The
maintainable React/TypeScript source lives in `module-web/`; its deterministic build writes the
ESM entry, lazy chunk and stylesheet consumed from `frontend/`. React itself is never bundled:
the entry activates against the single runtime injected by Union Web Shell. Builder
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
v2 maps canonical `/api/modules/sunshine/*` routes without a module-specific Core branch.

## Supervisor-supplied worker environment

```text
UNION_PLUGIN_ID=sunshine
UNION_PLUGIN_VERSION=0.6.0
UNION_PLUGIN_BIND=127.0.0.1:18104
UNION_PLUGIN_PORT=18104
UNION_PLUGIN_PACKAGE_ROOT=/opt/union/releases/<release>/modules/sunshine
UNION_PLUGIN_CONFIG=/var/lib/union/plugins/config/sunshine.json
UNION_MODULE_PROTOCOL=gateway-v1
UNION_MODULE_AUDIENCE=sunshine
UNION_MODULE_TOKEN=<64 lowercase hexadecimal characters; supplied by Union>
UNION_MODULE_PREFIX=/api/modules/sunshine
```

Operators do not launch this binary with that block. They store values through the module's
`config/schema.json`; Runtime writes one schema-validated private JSON file and supplies only the
standard runtime context above. Database URL, credential key/id and production mode are read from
that file. No `SUNSHINE_*` environment aliases are accepted.

The PostgreSQL administrator must create schema `sunshine` owned by the module
role. Startup applies only migrations from `migrations/`; it deliberately does
not create roles, databases or schemas.

## Fresh 0.6 state

Sunshine 0.6 accepts only its current Manifest v2 configuration, PostgreSQL schema and
`sunshine:v1` module ciphertext. The worker contains no Union SQLite importer, old ciphertext
decoder, verification batch or rollback command. Deploy it with a fresh module database and enter
hosts again through Union.

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
cd module-web
npm ci
npm test
```

The module frontend retains the original Union card-and-adjacent-panel workflow: inline host
editing, application and client management, PIN pairing, JSON configuration, system operations
and per-host logs. All browser calls use the Manifest-owned
`/api/modules/sunshine` gateway namespace; write controls are gated by module RBAC.

This source repository may be versioned and built independently for composition
purposes, but only the complete Union distribution is an operator-facing release.
