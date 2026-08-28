# Security policy

## Reporting a vulnerability

Do not disclose security vulnerabilities in a public issue. Use GitHub
[private vulnerability reporting](https://github.com/isarmg/sunshine-worker/security/advisories/new).
If that channel is unavailable, open an issue that asks for a private contact
without including vulnerability details.

Include the affected revision, reproduction steps and expected impact. The
maintainer aims to acknowledge reports within 72 hours and provide an initial
assessment within seven days.

## Supported boundary

Only the revision included in the current Union release receives security
fixes. This repository does not publish a standalone operator-facing service.

The worker must bind to loopback, is supervised by Union, and accepts only the
process-scoped `gateway-v1` contract. Union is the sole public ingress and owns
browser authentication, RBAC, CSRF protection, route authorization and request
identity. The internal gateway token authenticates Union to this worker; it is
not an end-user credential.

The module owns its PostgreSQL database/schema, migrations and encryption key.
It must not receive Core database access, browser cookies or another module's
storage. The process boundary provides lifecycle and failure isolation, not an
OS sandbox: official Builder-bundled modules remain trusted release code.
