import { createSarmgReactViteConfig } from "@sarmg/web-toolchain/vite";
import { mergeConfig } from "vite";
import { fileURLToPath } from "node:url";

export default mergeConfig(createSarmgReactViteConfig(), { resolve: { alias: [
  { find: /^@sarmg\/admin-ui$/, replacement: fileURLToPath(new URL("./shell/ui.js", import.meta.url)) },
] } });
