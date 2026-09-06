import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import { promisify } from "node:util";
import { mkdtemp, readFile, rm } from "node:fs/promises";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { createServer } from "node:https";
import { randomBytes } from "node:crypto";
import { chromium, expect } from "@playwright/test";
import { withLocalServer } from "./local-server.mjs";

// Real Manager + isolated database + a TLS-verified Sunshine protocol fixture.
// No installed Sunshine instance, user data, system trust store or live service is changed.
const root = await mkdtemp(join(tmpdir(), "sarmg-sunshine-tls-"));
let upstream;
try {
  const key = join(root, "server.key"); const cert = join(root, "server.crt");
  const caKey = join(root, "ca.key"); const caCert = join(root, "ca.crt"); const csr = join(root, "server.csr");
  await promisify(execFile)("openssl", ["req", "-x509", "-newkey", "rsa:2048", "-nodes", "-keyout", caKey, "-out", caCert, "-days", "1", "-subj", "/CN=Sunshine test CA", "-addext", "basicConstraints=critical,CA:TRUE"]);
  await promisify(execFile)("openssl", ["req", "-new", "-newkey", "rsa:2048", "-nodes", "-keyout", key, "-out", csr, "-subj", "/CN=localhost", "-addext", "subjectAltName=DNS:localhost,IP:127.0.0.1", "-addext", "basicConstraints=critical,CA:FALSE", "-addext", "extendedKeyUsage=serverAuth"]);
  await promisify(execFile)("openssl", ["x509", "-req", "-in", csr, "-CA", caCert, "-CAkey", caKey, "-CAcreateserial", "-out", cert, "-days", "1", "-copy_extensions", "copy"]);
  let config = { sunshine_name: "Fixture Sunshine", output_name: "DISPLAY1", custom_setting: "preserved" };
  let apps = [{ name: "Desktop", cmd: "", "prep-cmd": [{ do: "prepare", undo: "restore" }] }];
  let clients = [{ name: "Moonlight", uuid: "fixture-client", enabled: true }];
  const mutations = []; const unexpected = [];
  upstream = createServer({ key: await readFile(key), cert: await readFile(cert) }, async (request, response) => {
    const json = value => { response.setHeader("content-type", "application/json"); response.end(JSON.stringify(value)); };
    if (request.url !== "/api/configLocale" && request.headers.authorization !== `Basic ${Buffer.from("fixture:fixture-password").toString("base64")}`) {
      response.statusCode = 401; json({ status: false }); return;
    }
    let text = ""; for await (const chunk of request) text += chunk;
    const body = text ? JSON.parse(text) : undefined;
    if (request.method === "GET") {
      if (request.url === "/api/config") return json({ status: true, platform: "windows", version: "fixture-current", ...config });
      if (request.url === "/api/apps") return json({ apps });
      if (request.url === "/api/clients/list") return json({ status: true, named_certs: clients });
      if (request.url === "/api/configLocale") return json({ status: true, locale: "zh" });
      if (request.url === "/api/logs") { response.setHeader("content-type", "text/plain"); response.end("TLS fixture log\n"); return; }
    } else {
      mutations.push({ path: request.url, method: request.method, body });
      if (request.url === "/api/config") { config = body; return json({ status: true }); }
      if (request.url === "/api/apps") { const { index, ...app } = body; if (index === -1) apps.push(app); else apps[index] = app; return json({ status: true }); }
      if (/^\/api\/apps\/\d+$/.test(request.url) && request.method === "DELETE") { apps.splice(Number(request.url.split("/").at(-1)), 1); return json({ status: true }); }
      if (request.url === "/api/clients/update") { clients[0].enabled = body.enabled; return json({ status: true }); }
      if (request.url === "/api/clients/unpair" || request.url === "/api/clients/unpair-all") { clients = []; return json({ status: true }); }
      if (request.url === "/api/pin" && body.pin === "0000") return json({ status: false });
      if (request.url === "/api/pin" && body.pin === "9999") return json({ malformed: true });
      if (["/api/pin", "/api/restart", "/api/reset-display-device-persistence", "/api/apps/close"].includes(request.url)) return json({ status: true });
    }
    unexpected.push(`${request.method} ${request.url}`); response.statusCode = 404; json({ status: false });
  });
  await new Promise(done => upstream.listen(0, "127.0.0.1", done));
  await withLocalServer({ prefix: "SUNSHINE_MANAGER", binary: "../../target/debug/sunshine-manager", extraEnv: {
    SUNSHINE_MANAGER_PRODUCTION: "false", SUNSHINE_MANAGER_CREDENTIAL_KEY: randomBytes(32).toString("base64"), SUNSHINE_MANAGER_CREDENTIAL_KEY_ID: "test",
    SSL_CERT_FILE: caCert,
  } }, async ({ base, password }) => {
    const browser = await chromium.launch();
    const failures = [];
    const reads = [];
    let page;
    try {
      page = await browser.newPage(); const errors = []; page.on("pageerror", error => errors.push(error.message));
      page.on("response", response => { if (response.status() >= 400) failures.push(`${response.status()} ${new URL(response.url()).pathname}`); });
      page.on("response", async response => {
        if (response.url().endsWith("/apps") && response.request().method() === "GET") {
          try { const value = await response.json(); reads.push({ apps: value.apps?.length }); } catch { /* request was cancelled */ }
        }
      });
      page.on("requestfailed", request => failures.push(`${request.failure()?.errorText} ${new URL(request.url()).pathname}`));
      await page.goto(base); await page.getByLabel("Username", { exact: true }).fill("admin"); await page.getByLabel("Password", { exact: true }).fill(password);
      await page.getByRole("button", { name: "Sign in", exact: true }).click();
      await page.getByRole("button", { name: "新建实例", exact: true }).click();
      await page.getByLabel("实例名称", { exact: true }).fill("Sunshine TLS 测试"); await page.getByLabel("主机地址", { exact: true }).fill("127.0.0.1");
      await page.getByLabel("Web 端口", { exact: true }).fill(String(upstream.address().port)); await page.getByLabel("Sunshine 用户名", { exact: true }).fill("fixture");
      await page.getByLabel("Sunshine 密码", { exact: true }).fill("fixture-password"); await page.getByRole("button", { name: "创建实例", exact: true }).click();
      const tab = name => page.getByRole("navigation", { name: "Sunshine 实例功能" }).getByRole("button", { name, exact: true });
      const confirm = async title => { await page.getByRole("dialog", { name: title, exact: true }).getByRole("button", { name: "Confirm", exact: true }).click(); };
      let lastOperation;
      const completed = async (mutation, expected = "执行成功") => {
        const received = page.waitForResponse(response => ["POST", "DELETE"].includes(response.request().method()) && response.status() === 202);
        await mutation(); const operation = await (await received).json();
        assert.match(operation.operation_id, /^op_[0-9a-f-]{36}$/); assert.notEqual(operation.operation_id, lastOperation); lastOperation = operation.operation_id;
        await expect(page.getByRole("region", { name: "远端操作状态" })).toContainText(lastOperation);
        await expect(page.getByRole("region", { name: "远端操作状态" })).toContainText(expected, { timeout: 15000 });
      };
      const configResponse = page.waitForResponse(response => response.url().endsWith("/config"));
      await tab("Sunshine 配置").click(); assert.equal((await configResponse).status(), 200, "TLS fixture configuration request");
      await page.getByLabel("搜索配置", { exact: true }).fill("sunshine_name");
      await page.getByLabel("Sunshine 名称 (sunshine_name)", { exact: true }).fill("Saved through real Manager");
      await page.getByRole("button", { name: "保存 Sunshine 配置", exact: true }).click(); await completed(() => confirm("保存远端配置"));
      assert.equal(config.sunshine_name, "Saved through real Manager"); assert.equal(config.custom_setting, "preserved"); assert.ok(!Object.hasOwn(config, "platform"));
      await tab("应用管理").click(); await page.getByRole("button", { name: "编辑应用 1", exact: true }).click();
      await page.getByLabel("应用名称", { exact: true }).fill("Edited Desktop"); await page.getByRole("button", { name: "保存应用", exact: true }).click();
      await completed(() => page.getByRole("button", { name: "确认保存应用", exact: true }).click());
      assert.equal(apps[0].name, "Edited Desktop"); assert.deepEqual(apps[0]["prep-cmd"], [{ do: "prepare", undo: "restore" }]);
      await page.getByRole("button", { name: "新增应用", exact: true }).click(); await page.getByLabel("应用名称", { exact: true }).fill("New Game");
      await page.getByRole("button", { name: "保存应用", exact: true }).click(); await completed(() => page.getByRole("button", { name: "确认保存应用", exact: true }).click());
      assert.equal(apps.length, 2); await page.getByRole("button", { name: "删除应用 2", exact: true }).click(); await completed(() => confirm("删除应用")); assert.equal(apps.length, 1);
      await tab("客户端与配对").click(); await page.getByRole("button", { name: "禁用客户端", exact: true }).click(); await completed(() => confirm("禁用客户端")); assert.equal(clients[0].enabled, false);
      await page.getByLabel("客户端名称", { exact: true }).fill("Paired client"); await page.getByLabel("配对 PIN", { exact: true }).fill("1234");
      await completed(() => page.getByRole("button", { name: "提交配对", exact: true }).click()); await expect(page.getByLabel("配对 PIN", { exact: true })).toHaveValue("");
      await page.getByLabel("配对 PIN", { exact: true }).fill("0000");
      await completed(() => page.getByRole("button", { name: "提交配对", exact: true }).click(), "执行失败");
      await page.getByLabel("配对 PIN", { exact: true }).fill("9999");
      await completed(() => page.getByRole("button", { name: "提交配对", exact: true }).click(), "结果不确定");
      await page.getByRole("button", { name: "已核对：实际失败", exact: true }).click(); await confirm("确认人工核对结果");
      await expect(page.getByRole("region", { name: "远端操作状态" })).toContainText("已人工核对");
      await page.getByRole("button", { name: "解除配对", exact: true }).click(); await completed(() => confirm("解除客户端配对")); assert.equal(clients.length, 0);
      await tab("日志").click(); await expect(page.locator("pre")).toHaveText("TLS fixture log\n");
      await tab("服务操作").click(); await page.getByRole("button", { name: "重启 Sunshine", exact: true }).click(); await completed(() => confirm("重启 Sunshine"));
      await page.getByRole("button", { name: "重置显示设备", exact: true }).click(); await completed(() => confirm("重置显示设备"));
      assert.deepEqual(unexpected, []); assert.deepEqual(errors, []); assert.equal(mutations.length, 11);
      await page.reload(); await tab("服务操作").click();
      await page.getByText("按操作 ID 查询（仅当前管理员自己的任务）", { exact: true }).click(); await page.getByLabel("操作 ID", { exact: true }).fill(lastOperation);
      await page.getByRole("button", { name: "查询操作", exact: true }).click(); await expect(page.getByRole("region", { name: "远端操作状态" })).toContainText("执行成功");
      console.log("Real Manager + trusted HTTPS Sunshine fixture: configuration, app CRUD, client permissions/pairing/unpairing, logs, restart/display reset, durable operations, false/malformed 2xx rejection, human resolution and reload lookup passed");
    } catch (error) {
      console.error("Fixture request failures:", failures, "application reads:", reads);
      if (page) { await page.screenshot({ path: "/tmp/sunshine-live-remote-failure.png", fullPage: true }); }
      throw error;
    }
    finally { await browser.close(); }
  });
} finally {
  if (upstream) await new Promise(done => upstream.close(done));
  await rm(root, { recursive: true });
}
