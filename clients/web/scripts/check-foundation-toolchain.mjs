import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

import { assertAdministratorWebToolchain } from "@sarmg/admin-web";
import manifest from "../package.json" with { type: "json" };

const lock = JSON.parse(
  readFileSync(new URL("../package-lock.json", import.meta.url), "utf8"),
);
const foundationReleaseBase =
  "https://github.com/isarmg/sarmg-foundation/releases/download/v0.3.0";
const foundationVersion = "0.3.0";
const foundationPackages = ["admin-web", "contracts", "design-tokens", "http-client"];

const nodeVersion = readFileSync(
  new URL("../../../.node-version", import.meta.url),
  "utf8",
);
assert.match(nodeVersion, /^26\.7\.0\n?$/);
assertAdministratorWebToolchain(manifest, nodeVersion);
for (const name of foundationPackages) {
  const dependency = `@sarmg/${name}`;
  const expected = `${foundationReleaseBase}/sarmg-${name}-${foundationVersion}.tgz`;
  assert.equal(manifest.dependencies?.[dependency], expected);
  assert.equal(lock.packages?.[""]?.dependencies?.[dependency], expected);

  const locked = lock.packages?.[`node_modules/${dependency}`];
  assert.equal(locked?.version, foundationVersion);
  assert.equal(locked?.resolved, expected);
  assert.match(locked?.integrity ?? "", /^sha512-[A-Za-z0-9+/]+={0,2}$/);
}
