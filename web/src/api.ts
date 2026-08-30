export type SessionInfo = {
  authenticated: true;
  user_id: string;
  email: string;
  csrf_token: string;
};

let csrfToken: string | undefined;

export async function requestJson<T>(url: string, init?: RequestInit): Promise<T> {
  const method = (init?.method ?? "GET").toUpperCase();
  const mutation = !["GET", "HEAD", "OPTIONS", "TRACE"].includes(method);
  const response = await fetch(url, {
    credentials: "same-origin",
    ...init,
    headers: {
      "content-type": "application/json",
      ...(mutation && csrfToken ? { "x-csrf-token": csrfToken } : {}),
      ...(init?.headers ?? {}),
    },
  });
  if (response.status === 204) return undefined as T;
  const body = await response.json().catch(() => null);
  if (!response.ok) throw new Error(body?.message || response.statusText);
  return body as T;
}

export async function login(email: string, password: string): Promise<SessionInfo> {
  const session = await requestJson<SessionInfo>("/api/v1/auth/login", {
    method: "POST",
    body: JSON.stringify({ email, password }),
  });
  csrfToken = session.csrf_token;
  return session;
}

export async function restoreSession(): Promise<SessionInfo> {
  const session = await requestJson<SessionInfo>("/api/v1/auth/session");
  csrfToken = session.csrf_token;
  return session;
}

export async function logout() {
  try {
    await requestJson("/api/v1/auth/logout", { method: "POST" });
  } finally {
    csrfToken = undefined;
  }
}
