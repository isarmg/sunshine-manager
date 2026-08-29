// src/platform.ts
var activeApi = null;
function bindModuleApi(api) {
  if (activeApi && activeApi.basePath !== api.basePath) {
    throw new Error("\u6A21\u5757 API \u4E0D\u80FD\u8DE8\u8D8A Manifest \u547D\u540D\u7A7A\u95F4\u91CD\u65B0\u7ED1\u5B9A");
  }
  activeApi = api;
}
function request(path, init) {
  if (!activeApi) return Promise.reject(new Error("\u6A21\u5757\u5C1A\u672A\u7531 Union Web Shell \u6FC0\u6D3B"));
  return activeApi.request(path, init);
}
function pathSegment(value) {
  return encodeURIComponent(String(value));
}

// src/runtime.ts
var injected = null;
function bindReact(runtime) {
  if (injected && injected !== runtime) {
    throw new Error("\u6A21\u5757 React Runtime \u4E0D\u80FD\u5728\u6FC0\u6D3B\u540E\u66FF\u6362");
  }
  injected = runtime;
}
function react() {
  if (!injected) throw new Error("\u6A21\u5757\u5C1A\u672A\u7531 Union Web Shell \u6FC0\u6D3B");
  return injected;
}
var Fragment = Symbol.for("react.fragment");
var createElement = ((...args) => react().createElement(...args));
var createContext = ((...args) => react().createContext(...args));
var forwardRef = ((...args) => react().forwardRef(...args));
var useCallback = ((...args) => react().useCallback(...args));
var useContext = ((...args) => react().useContext(...args));
var useEffect = ((...args) => react().useEffect(...args));
var useId = ((...args) => react().useId(...args));
var useLayoutEffect = ((...args) => react().useLayoutEffect(...args));
var useMemo = ((...args) => react().useMemo(...args));
var useRef = ((...args) => react().useRef(...args));
var useState = ((...args) => react().useState(...args));
var useSyncExternalStore = ((...args) => react().useSyncExternalStore(...args));

// src/features/sunshine/api.ts
var sunshineHostPath = (id) => `/hosts/${pathSegment(id)}`;
var MAX_COLLECTION_ITEMS = 512;
var MAX_OBJECT_KEYS = 256;
var MAX_DISPLAY_TEXT_CHARACTERS = 1024;
var MAX_CLIENT_ID_CHARACTERS = 128;
function isRecord(value) {
  return Boolean(value) && typeof value === "object" && !Array.isArray(value);
}
function isBoundedText(value, maxCharacters) {
  let characters = 0;
  for (const character of value) {
    characters += 1;
    if (characters > maxCharacters) return false;
    const code = character.charCodeAt(0);
    if (code <= 31 || code >= 127 && code <= 159) return false;
  }
  return true;
}
function hasSafeObjectShape(value) {
  const keys = Object.keys(value);
  return keys.length <= MAX_OBJECT_KEYS && keys.every((key) => isBoundedText(key, MAX_CLIENT_ID_CHARACTERS));
}
function isSafeDisplayText(value) {
  return typeof value === "string" && isBoundedText(value, MAX_DISPLAY_TEXT_CHARACTERS);
}
function parseSunshineAppsResponse(value) {
  if (!isRecord(value) || !Array.isArray(value.apps) || value.apps.length > MAX_COLLECTION_ITEMS) {
    throw new Error("Sunshine \u5E94\u7528\u5217\u8868\u54CD\u5E94\u683C\u5F0F\u65E0\u6548");
  }
  const apps = value.apps.map((item, index) => {
    if (!isRecord(item) || !hasSafeObjectShape(item) || !isSafeDisplayText(item.name) || !(item.cmd === void 0 || item.cmd === null || typeof item.cmd === "string")) {
      throw new Error("Sunshine \u5E94\u7528\u5217\u8868\u54CD\u5E94\u683C\u5F0F\u65E0\u6548");
    }
    return { ...item, name: item.name, cmd: item.cmd, index };
  });
  return { apps };
}
function parseSunshineClientsResponse(value) {
  if (!isRecord(value) || typeof value.status !== "boolean" || !Array.isArray(value.named_certs) || value.named_certs.length > MAX_COLLECTION_ITEMS) {
    throw new Error("Sunshine \u5BA2\u6237\u7AEF\u5217\u8868\u54CD\u5E94\u683C\u5F0F\u65E0\u6548");
  }
  const uuids = /* @__PURE__ */ new Set();
  const named_certs = value.named_certs.map((item) => {
    if (!isRecord(item) || !hasSafeObjectShape(item) || typeof item.uuid !== "string" || !isBoundedText(item.uuid, MAX_CLIENT_ID_CHARACTERS) || item.uuid.trim() !== item.uuid || !item.uuid || typeof item.enabled !== "boolean" || !(item.name === void 0 || item.name === null || isSafeDisplayText(item.name)) || uuids.has(item.uuid)) {
      throw new Error("Sunshine \u5BA2\u6237\u7AEF\u5217\u8868\u54CD\u5E94\u683C\u5F0F\u65E0\u6548");
    }
    uuids.add(item.uuid);
    return {
      ...item,
      name: item.name,
      uuid: item.uuid,
      enabled: item.enabled
    };
  });
  return { status: value.status, named_certs };
}
var sunshineApi = {
  sunshineHosts: (signal) => request("/hosts", { signal }),
  sunshineCreateHost: (body) => request(
    "/hosts",
    { method: "POST", body: JSON.stringify(body), expectedStatus: 201 }
  ),
  sunshineUpdateHost: (id, body) => request(
    sunshineHostPath(id),
    { method: "PATCH", body: JSON.stringify(body) }
  ),
  sunshineDeleteHost: (id) => request(sunshineHostPath(id), { method: "DELETE", expectedStatus: 204 }),
  sunshineApiLogs: (id) => request(`${sunshineHostPath(id)}/api-logs`),
  sunshineApps: async (id) => parseSunshineAppsResponse(
    await request(`${sunshineHostPath(id)}/apps`)
  ),
  sunshineSaveApp: (id, app) => request(
    `${sunshineHostPath(id)}/apps`,
    { method: "POST", body: JSON.stringify(app) }
  ),
  sunshineCloseApp: (id) => request(`${sunshineHostPath(id)}/apps/close`, { method: "POST" }),
  sunshineDeleteApp: (id, index) => request(
    `${sunshineHostPath(id)}/apps/${pathSegment(index)}`,
    { method: "DELETE" }
  ),
  sunshineClients: async (id) => parseSunshineClientsResponse(
    await request(`${sunshineHostPath(id)}/clients`)
  ),
  sunshineUnpairClient: (id, uuid) => request(
    `${sunshineHostPath(id)}/clients/unpair`,
    { method: "POST", body: JSON.stringify({ uuid }) }
  ),
  sunshineUnpairAll: (id) => request(`${sunshineHostPath(id)}/clients/unpair-all`, { method: "POST" }),
  sunshineUpdateClient: (id, uuid, enabled) => request(
    `${sunshineHostPath(id)}/clients/update`,
    { method: "POST", body: JSON.stringify({ uuid, enabled }) }
  ),
  sunshineConfig: (id) => request(`${sunshineHostPath(id)}/config`),
  sunshineSaveConfig: (id, config) => request(
    `${sunshineHostPath(id)}/config`,
    { method: "POST", body: JSON.stringify(config) }
  ),
  sunshinePin: (id, pin, name) => request(
    `${sunshineHostPath(id)}/pin`,
    { method: "POST", body: JSON.stringify({ pin, name }) }
  ),
  sunshineRestart: (id) => request(`${sunshineHostPath(id)}/restart`, { method: "POST" }),
  sunshineResetDisplay: (id) => request(`${sunshineHostPath(id)}/reset-display`, { method: "POST" })
};

// src/features/sunshine/data.ts
var OPTIMISTIC_HOST_ID_PREFIX = "optimistic-sunshine-host:";
var optimisticHostSequence = 0;
var sunshineHostMutationKeys = {
  create: ["sunshine-host-mutation", "create"],
  update: ["sunshine-host-mutation", "update"],
  delete: ["sunshine-host-mutation", "delete"]
};
function sunshineLogLines(value) {
  return value.content.split(/\r?\n/);
}
function parseSunshineConfigDraft(text) {
  const value = JSON.parse(text);
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    throw new Error("Sunshine \u914D\u7F6E\u5FC5\u987B\u662F JSON \u5BF9\u8C61");
  }
  return value;
}
function optimisticSunshineHost(request2) {
  optimisticHostSequence += 1;
  const host = request2.host.trim();
  const urlHost = host.includes(":") && !host.startsWith("[") ? `[${host}]` : host;
  return {
    id: `${OPTIMISTIC_HOST_ID_PREFIX}${Date.now()}:${optimisticHostSequence}`,
    name: request2.name.trim(),
    host,
    web_port: request2.web_port,
    username: request2.username.trim(),
    password_set: Boolean(request2.password),
    verify_tls: request2.verify_tls,
    web_url: `https://${urlHost}:${request2.web_port}`,
    probe_status: "pending",
    reachable: null,
    connected: null,
    connection_error: "\u6B63\u5728\u4FDD\u5B58\u5E76\u68C0\u6D4B\u8FDE\u63A5\u2026"
  };
}
function isOptimisticSunshineHost(host) {
  return host.id.startsWith(OPTIMISTIC_HOST_ID_PREFIX);
}
function persistedSunshineHosts(hosts) {
  return hosts.filter((host) => !isOptimisticSunshineHost(host));
}
function applySunshineHostPatch(host, patch) {
  const next = { ...host };
  if (typeof patch.name === "string") next.name = patch.name;
  if (typeof patch.host === "string") next.host = patch.host;
  if (typeof patch.web_port === "number") next.web_port = patch.web_port;
  if (typeof patch.username === "string") next.username = patch.username;
  if (typeof patch.verify_tls === "boolean") next.verify_tls = patch.verify_tls;
  if (Object.hasOwn(patch, "password")) next.password_set = Boolean(patch.password);
  if (typeof patch.host === "string" || typeof patch.web_port === "number") {
    const urlHost = next.host.includes(":") && !next.host.startsWith("[") ? `[${next.host}]` : next.host;
    next.web_url = `https://${urlHost}:${next.web_port}`;
  }
  if (["host", "web_port", "username", "password", "verify_tls"].some((key) => Object.hasOwn(patch, key))) {
    next.probe_status = "pending";
    next.reachable = null;
    next.connected = null;
    next.connection_error = "\u6B63\u5728\u4FDD\u5B58\u5E76\u68C0\u6D4B\u8FDE\u63A5\u2026";
  }
  return next;
}
function mergeSunshineHostSnapshot(remote, current, deletingIds, updateOverlays = [], createdHosts = []) {
  const next = remote.filter((host) => !deletingIds.has(host.id));
  const ids = new Set(next.map((host) => host.id));
  for (const host of current) {
    if (isOptimisticSunshineHost(host) && !deletingIds.has(host.id) && !ids.has(host.id)) {
      next.push(host);
      ids.add(host.id);
    }
  }
  for (const created of createdHosts) {
    if (deletingIds.has(created.id)) continue;
    const index = next.findIndex((host) => host.id === created.id);
    if (index >= 0) next[index] = created;
    else {
      next.push(created);
      ids.add(created.id);
    }
  }
  for (const overlay of updateOverlays) {
    if (deletingIds.has(overlay.id)) continue;
    const index = next.findIndex((host) => host.id === overlay.id);
    const base = index >= 0 ? next[index] : current.find((host) => host.id === overlay.id);
    if (!base) continue;
    const updated = overlay.saved ?? applySunshineHostPatch(base, overlay.patch);
    if (index >= 0) next[index] = updated;
    else {
      next.push(updated);
      ids.add(updated.id);
    }
  }
  return next;
}
function sunshineHostsRefetchInterval(hosts, mutationCanBeOverwritten = false) {
  if (mutationCanBeOverwritten || hosts?.some(isOptimisticSunshineHost)) return false;
  return hosts?.some((host) => host.probe_status === "pending") ? 1500 : 3e4;
}
function replaceSunshineHost(hosts, host, previousId = host.id) {
  const next = [...hosts];
  const previousIndex = next.findIndex((entry) => entry.id === previousId);
  const finalIndex = next.findIndex((entry) => entry.id === host.id);
  if (previousIndex >= 0) {
    next[previousIndex] = host;
    if (finalIndex >= 0 && finalIndex !== previousIndex) next.splice(finalIndex, 1);
    return next;
  }
  if (finalIndex >= 0) {
    next[finalIndex] = host;
    return next;
  }
  next.push(host);
  return next;
}
function removeSunshineHost(hosts, id) {
  return hosts.filter((host) => host.id !== id);
}
function restoreSunshineHost(hosts, host, originalIndex) {
  if (hosts.some((entry) => entry.id === host.id)) return [...hosts];
  const next = [...hosts];
  next.splice(Math.min(Math.max(originalIndex, 0), next.length), 0, host);
  return next;
}

export {
  bindModuleApi,
  bindReact,
  Fragment,
  createElement,
  createContext,
  forwardRef,
  useCallback,
  useContext,
  useEffect,
  useId,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  useSyncExternalStore,
  parseSunshineAppsResponse,
  parseSunshineClientsResponse,
  sunshineApi,
  sunshineHostMutationKeys,
  sunshineLogLines,
  parseSunshineConfigDraft,
  optimisticSunshineHost,
  isOptimisticSunshineHost,
  persistedSunshineHosts,
  applySunshineHostPatch,
  mergeSunshineHostSnapshot,
  sunshineHostsRefetchInterval,
  replaceSunshineHost,
  removeSunshineHost,
  restoreSunshineHost
};
