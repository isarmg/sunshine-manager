# Security policy

## Reporting a vulnerability

Do not disclose security vulnerabilities in a public issue. Use GitHub
[private vulnerability reporting](https://github.com/isarmg/sunshine-manager/security/advisories/new).
If that channel is unavailable, open an issue asking for a private contact without including
vulnerability details.

Include the affected release or revision, reproduction steps and expected impact. The maintainer
aims to acknowledge reports within 72 hours and provide an initial assessment within seven days.

## Supported boundary

Security fixes target the latest released Sunshine Manager version and the current `main` branch.
Sunshine Manager is a standalone service: it owns its browser authentication, administrator RBAC,
CSRF checks, exact-current SQLite schema, operation queue and audit outbox. It has no central
gateway, shared session service or PostgreSQL dependency.

The default listener is loopback-only. A production deployment must terminate HTTPS at a trusted
reverse proxy, preserve the original `Host` header and use secure session cookies. Development
cookies are accepted only with an explicit development setting and a loopback bind address. Do not
publish the application over cleartext HTTP or rely on untrusted forwarded-address headers.

The service stores only hashes of random browser session and CSRF tokens. Sunshine credentials and
durable operation requests are encrypted at rest with `SUNSHINE_MANAGER_CREDENTIAL_KEY`; protect
that key separately from the database, restrict both to the `isarmg-sunshine` account and include
the key identifier in recovery procedures. Losing the key makes encrypted host credentials and
pending requests unrecoverable. A database copy without the key is not a usable full backup.
The runtime accepts only the configured current key identifier. Key rotation and re-encryption are
offline upgrade operations; the product does not retain previous decryption keys.

Run one service process per SQLite database. The process holds an exclusive instance lock for its
full lifetime. The separate `isarmg-upgrade` tool shares the maintenance lock for online backup;
restore, version upgrade and administrator maintenance require its exclusive mode. The product
does not read old schemas or implement migration, backup or restore.

## External systems

Configured Sunshine hosts and their responses are untrusted network peers. Prefer HTTPS with
certificate verification enabled, restrict host addresses at the deployment firewall and avoid
credentials with privileges beyond the managed Sunshine instance. Disabling upstream TLS
verification removes server authentication and is not recommended for production.

Cover URLs must use HTTPS, match `SUNSHINE_MANAGER_COVER_URL_ALLOWLIST` and resolve only to public
addresses when accepted and immediately before dispatch. The managed Sunshine host performs the
actual fetch, so its own network must also block private, link-local, loopback and metadata
destinations and unsafe redirects. The manager-side allowlist is not a substitute for Sunshine
host egress controls.

An operation in `unknown` state may already have changed the remote Sunshine host. Investigate and
reconcile the remote state before retrying; do not automatically replay it with a new idempotency
key.
