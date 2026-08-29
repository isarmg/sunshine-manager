export async function requestJson<T>(url: string, init?: RequestInit): Promise<T> {
  const response = await fetch(url, {
    credentials: "same-origin",
    headers: { "content-type": "application/json", ...(init?.headers ?? {}) },
    ...init,
  });
  if (response.status === 204) return undefined as T;
  const body = await response.json().catch(() => null);
  if (!response.ok) throw new Error(body?.message || response.statusText);
  return body as T;
}

export async function login(email: string, password: string) {
  await requestJson("/api/v1/auth/login", {
    method: "POST",
    body: JSON.stringify({ email, password }),
  });
}

export async function logout() {
  await requestJson("/api/v1/auth/logout", { method: "POST" });
}
