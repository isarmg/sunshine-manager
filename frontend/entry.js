import {
  applySunshineHostPatch,
  bindModuleApi,
  bindReact,
  mergeSunshineHostSnapshot,
  optimisticSunshineHost,
  parseSunshineAppsResponse,
  parseSunshineClientsResponse,
  parseSunshineConfigDraft,
  sunshineApi
} from "./chunks/chunk-DMRF6SCT.js";

// src/entry.ts
var entry = {
  pluginApiVersion: "2.0.0",
  moduleId: "sunshine",
  version: "0.6.0",
  async activate(host) {
    bindReact(host.react);
    bindModuleApi(host.api);
    const module = await import("./chunks/app-YMJERJBC.js");
    return module.activate();
  }
};
var entry_default = entry;
export {
  applySunshineHostPatch,
  entry_default as default,
  mergeSunshineHostSnapshot,
  optimisticSunshineHost,
  parseSunshineAppsResponse,
  parseSunshineClientsResponse,
  parseSunshineConfigDraft,
  sunshineApi
};
