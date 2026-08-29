# Sunshine Web Module

This directory is the maintainable source for Sunshine's Union Web contribution. It restores the
last integrated Union interaction model without moving business UI back into Core:

- host cards with optimistic create, inline editing and deletion;
- the adjacent application/client/PIN/configuration/system panel;
- per-host Sunshine API logs;
- write and proxy controls gated by module permissions.

`src/entry.ts` is the small stable Plugin API entry. It first binds the React instance and
module-scoped API supplied by Union Web Shell, then lazily loads the application chunk. The lazy
boundary is intentional: React Query and Lucide may create React contexts during module
evaluation, so they must evaluate only after the Shell runtime is available. The build aliases
all `react` and JSX-runtime imports to that injected bridge; it must never bundle another React
or ReactDOM runtime.

`npm run build` type-checks the source and writes deterministic artifacts to both ignored
`dist/` and the repository package's `../frontend/`. The latter is committed because the
Manifest bundle is also valid without an optional Builder frontend step. CI rebuilds it and
requires a clean diff.

Use:

```console
npm ci
npm test
```
