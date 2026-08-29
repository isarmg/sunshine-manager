import { copyFile, rm } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { build } from "esbuild";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const outputs = [path.join(root, "dist"), path.resolve(root, "../frontend")];
const options = {
  absWorkingDir: root,
  entryPoints: { entry: "src/entry.ts", styles: "src/styles.css" },
  bundle: true,
  splitting: true,
  format: "esm",
  platform: "browser",
  target: ["es2022"],
  jsx: "transform",
  jsxFactory: "h",
  jsxFragment: "Fragment",
  alias: {
    "react/jsx-runtime": path.join(root, "src/jsx-runtime.ts"),
    "react/jsx-dev-runtime": path.join(root, "src/jsx-runtime.ts"),
    react: path.join(root, "src/runtime.ts"),
  },
  entryNames: "[name]",
  chunkNames: "chunks/[name]-[hash]",
  legalComments: "eof",
  logLevel: "info",
};

for (const outdir of outputs) {
  await rm(outdir, { recursive: true, force: true });
  await build({ ...options, outdir });
  await copyFile(
    path.join(root, "THIRD_PARTY_LICENSES.txt"),
    path.join(outdir, "THIRD_PARTY_LICENSES.txt"),
  );
}
