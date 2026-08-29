import type * as ReactRuntime from "react";
import { bindModuleApi, type ModuleApi } from "./platform";
import { bindReact } from "./runtime";

export {
  parseSunshineAppsResponse,
  parseSunshineClientsResponse,
  sunshineApi,
} from "./features/sunshine/api";
export {
  applySunshineHostPatch,
  mergeSunshineHostSnapshot,
  optimisticSunshineHost,
  parseSunshineConfigDraft,
} from "./features/sunshine/data";

interface HostSdk {
  react: typeof ReactRuntime;
  api: ModuleApi;
}

const entry = {
  pluginApiVersion: "2.0.0",
  moduleId: "sunshine",
  version: "0.6.0",
  async activate(host: HostSdk) {
    bindReact(host.react);
    bindModuleApi(host.api);
    const module = await import("./app");
    return module.activate();
  },
};

export default entry;
