import { createAdministratorApiClient } from "@sarmg/admin-web";
import { isErrorEnvelope, type ErrorEnvelope } from "@sarmg/contracts";
import { isApiClientError } from "@sarmg/http-client";

export type HostInfo = {
  id: string;
  name: string;
  host: string;
  web_port: number;
  username: string;
  password_set: boolean;
  web_url: string;
  probe_status: "pending" | "complete";
  reachable: boolean | null;
  connected: boolean | null;
  connection_error: string | null;
};

export const CURRENT_API_PREFIX = "/api/v2";

export const adminApi = createAdministratorApiClient();

export async function requestJson<T>(
  path: string,
  guard: (value: unknown) => value is T,
  init: RequestInit = {},
): Promise<T> {
  return adminApi.request(path, guard, init);
}

export function currentErrorEnvelope(error: unknown): ErrorEnvelope | undefined {
  if (!isApiClientError(error)) return undefined;
  return isErrorEnvelope(error.envelope) ? error.envelope : undefined;
}

export function isHostInfoArray(value: unknown): value is HostInfo[] {
  return Array.isArray(value) && value.every(isHostInfo);
}

function isHostInfo(value: unknown): value is HostInfo {
  return (
    isExactRecord(value, [
      "id",
      "name",
      "host",
      "web_port",
      "username",
      "password_set",
      "web_url",
      "probe_status",
      "reachable",
      "connected",
      "connection_error",
    ]) &&
    isNonEmptyString(value.id) &&
    typeof value.name === "string" &&
    isNonEmptyString(value.host) &&
    Number.isInteger(value.web_port) &&
    Number(value.web_port) >= 1 &&
    Number(value.web_port) <= 65_535 &&
    typeof value.username === "string" &&
    typeof value.password_set === "boolean" &&
    isNonEmptyString(value.web_url) &&
    (value.probe_status === "pending" || value.probe_status === "complete") &&
    isNullableBoolean(value.reachable) &&
    isNullableBoolean(value.connected) &&
    (value.connection_error === null || typeof value.connection_error === "string")
  );
}

function isExactRecord(
  value: unknown,
  expectedKeys: readonly string[],
): value is Record<string, unknown> {
  if (typeof value !== "object" || value === null || Array.isArray(value)) return false;
  const actualKeys = Object.keys(value).sort();
  const expected = [...expectedKeys].sort();
  return (
    actualKeys.length === expected.length &&
    actualKeys.every((key, index) => key === expected[index])
  );
}

function isNonEmptyString(value: unknown): value is string {
  return typeof value === "string" && value.length > 0;
}

function isNullableBoolean(value: unknown): value is boolean | null {
  return value === null || typeof value === "boolean";
}
