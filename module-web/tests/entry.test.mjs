import assert from "node:assert/strict";
import { readFile, readdir } from "node:fs/promises";
import path from "node:path";
import test from "node:test";
import * as React from "react";
import { renderToStaticMarkup } from "react-dom/server";
import entry, {
  optimisticSunshineHost,
  parseSunshineAppsResponse,
  parseSunshineClientsResponse,
  parseSunshineConfigDraft,
  sunshineApi,
} from "../dist/entry.js";

test("compiled module activates against the Shell-owned React runtime", async () => {
  const activation = await entry.activate({
    react: React,
    api: { basePath: "/api/modules/sunshine", request: async () => [] },
  });

  assert.equal(entry.moduleId, "sunshine");
  assert.deepEqual(Object.keys(activation.components), ["SunshineView", "SunshineLogsView"]);
  assert.deepEqual(activation.primaryActions, [{
    component: "SunshineView",
    label: "添加 Sunshine 主机",
    permission: "sunshine.hosts.write",
  }]);
  const markup = renderToStaticMarkup(React.createElement(activation.components.SunshineView, {
    actionRequest: 0,
    onActionRequestHandled: () => undefined,
    hasPermission: () => true,
  }));
  assert.match(markup, /实例/);
});

test("Sunshine collection responses retain mutation identifiers and reject malformed data", () => {
  assert.deepEqual(parseSunshineAppsResponse({
    apps: [{ name: "Desktop", cmd: "" }, { name: "Game", cmd: "game.exe" }],
  }).apps.map(({ name, index }) => ({ name, index })), [
    { name: "Desktop", index: 0 },
    { name: "Game", index: 1 },
  ]);
  assert.throws(() => parseSunshineAppsResponse({ apps: [{ name: 12 }] }), /格式无效/);
  assert.throws(() => parseSunshineClientsResponse({
    status: true,
    named_certs: [
      { uuid: "same", enabled: true },
      { uuid: "same", enabled: false },
    ],
  }), /格式无效/);
});

test("draft and optimistic host helpers preserve the original interaction contract", () => {
  assert.deepEqual(parseSunshineConfigDraft('{"mode":"desktop"}'), { mode: "desktop" });
  assert.throws(() => parseSunshineConfigDraft("[]"), /JSON 对象/);
  const host = optimisticSunshineHost({
    name: " Living room ",
    host: "2001:db8::2",
    web_port: 47990,
    username: " admin ",
    password: "secret",
    verify_tls: true,
  });
  assert.equal(host.name, "Living room");
  assert.equal(host.web_url, "https://[2001:db8::2]:47990");
  assert.equal(host.probe_status, "pending");
  assert.equal(host.password_set, true);
});

test("management operations stay below the module API base", async () => {
  const calls = [];
  await entry.activate({
    react: React,
    api: {
      basePath: "/api/modules/sunshine",
      request: async (path, init) => {
        calls.push([path, init]);
        if (init?.method === "PATCH") {
          return {
            id: "host-one", name: "Renamed", host: "127.0.0.1", web_port: 47990,
            username: "admin", password_set: false, verify_tls: true,
            web_url: "https://127.0.0.1:47990", probe_status: "pending",
            reachable: null, connected: null,
          };
        }
        return {};
      },
    },
  });
  await sunshineApi.sunshineUpdateHost("host/one", { name: "Renamed" });
  await sunshineApi.sunshinePin("host/one", "1234", "Moonlight");
  assert.equal(calls[0][0], "/hosts/host%2Fone");
  assert.equal(calls[0][1].method, "PATCH");
  assert.equal(calls[1][0], "/hosts/host%2Fone/pin");
  assert.equal(calls[1][1].method, "POST");
});

test("compiled output delegates React instead of embedding another React runtime", async () => {
  const files = await readdir(new URL("../dist/chunks/", import.meta.url));
  const javascript = ["entry.js", ...files.filter((file) => file.endsWith(".js"))
    .map((file) => path.join("chunks", file))];
  const source = (await Promise.all(javascript.map((file) =>
    readFile(new URL(`../dist/${file}`, import.meta.url), "utf8")))).join("\n");
  assert.doesNotMatch(source, /react\.production\.js|Invalid hook call|createRoot\(/);
});
