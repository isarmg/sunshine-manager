import { FormEvent, useEffect, useState } from "react";
import { login, logout, requestJson } from "./api";

type Host = Record<string, unknown>;

export default function App() {
  const [authenticated, setAuthenticated] = useState(false);
  const [hosts, setHosts] = useState<Host[]>([]);
  const [email, setEmail] = useState("admin@example.com");
  const [password, setPassword] = useState("");
  const [error, setError] = useState("");

  useEffect(() => {
    if (authenticated) {
      requestJson<Host[]>("/api/services/sunshine/hosts")
        .then(setHosts)
        .catch(() => setAuthenticated(false));
    }
  }, [authenticated]);

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    try {
      await login(email, password);
      setAuthenticated(true);
      setError("");
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : "login failed");
    }
  };

  const leave = async () => {
    await logout();
    setAuthenticated(false);
    setHosts([]);
  };

  if (!authenticated) {
    return (
      <main>
        <h1>Sunshine Manager</h1>
        <form onSubmit={submit}>
          <input
            type="email"
            value={email}
            onChange={(event) => setEmail(event.target.value)}
          />
          <input
            type="password"
            value={password}
            onChange={(event) => setPassword(event.target.value)}
          />
          <button type="submit">登录</button>
        </form>
        {error && <p>{error}</p>}
      </main>
    );
  }

  return (
    <main>
      <h1>Sunshine Manager</h1>
      <button onClick={leave}>退出</button>
      <pre>{JSON.stringify(hosts, null, 2)}</pre>
    </main>
  );
}
