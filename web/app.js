const loginForm = document.querySelector("#login");
const content = document.querySelector("#content");
const hosts = document.querySelector("#hosts");
const logout = document.querySelector("#logout");

async function json(url, options = {}) {
  const response = await fetch(url, {
    credentials: "same-origin",
    headers: { "content-type": "application/json", ...(options.headers || {}) },
    ...options,
  });
  if (response.status === 204) return null;
  const body = await response.json().catch(() => null);
  if (!response.ok) throw new Error(body?.message || response.statusText);
  return body;
}

async function refresh() {
  const session = await json("/api/v1/auth/session");
  if (!session?.authenticated) throw new Error("unauthorized");
  const data = await json("/api/services/sunshine/hosts");
  hosts.textContent = JSON.stringify(data, null, 2);
  content.hidden = false;
  loginForm.hidden = true;
}

loginForm.addEventListener("submit", async (event) => {
  event.preventDefault();
  const form = new FormData(loginForm);
  await json("/api/v1/auth/login", {
    method: "POST",
    body: JSON.stringify({
      email: form.get("email"),
      password: form.get("password"),
    }),
  });
  await refresh();
});

logout.addEventListener("click", async () => {
  await json("/api/v1/auth/logout", { method: "POST" });
  loginForm.hidden = false;
  content.hidden = true;
});

refresh().catch(() => {
  loginForm.hidden = false;
  content.hidden = true;
});
