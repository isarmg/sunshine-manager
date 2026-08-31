import { useAdministratorSession } from "@sarmg/admin-web/react";
import { useEffect, useState } from "react";
import type { FormEvent } from "react";
import {
  CURRENT_API_PREFIX,
  adminApi,
  currentErrorEnvelope,
  isHostInfoArray,
  requestJson,
  type HostInfo,
} from "./api";

export default function App() {
  const authentication = useAdministratorSession(adminApi);
  const [hosts, setHosts] = useState<HostInfo[]>([]);
  const [username, setUsername] = useState("admin");
  const [password, setPassword] = useState("");
  const [error, setError] = useState("");

  useEffect(() => {
    if (authentication.phase !== "authenticated") {
      setHosts([]);
      setError("");
      return;
    }
    let cancelled = false;
    requestJson(`${CURRENT_API_PREFIX}/sunshine/hosts`, isHostInfoArray)
      .then((received) => {
        if (!cancelled) setHosts(received);
      })
      .catch((cause: unknown) => {
        if (!cancelled) {
          setError(currentErrorEnvelope(cause)?.message ?? "无法加载主机列表");
        }
      });
    return () => {
      cancelled = true;
    };
  }, [authentication.phase]);

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    try {
      await authentication.login(username, password);
      setError("");
    } catch (cause) {
      setError(currentErrorEnvelope(cause)?.message ?? "登录失败");
    }
  };

  const leave = async () => {
    try {
      await authentication.logout();
      setError("");
    } catch (cause) {
      setError(currentErrorEnvelope(cause)?.message ?? "退出失败");
    }
  };

  if (authentication.phase === "loading") {
    return <main>正在检查会话…</main>;
  }

  if (authentication.phase === "error") {
    return (
      <main>
        <h1>Sunshine Manager</h1>
        <p>{currentErrorEnvelope(authentication.error)?.message ?? "无法恢复会话"}</p>
        <button onClick={() => void authentication.restore()}>重试</button>
      </main>
    );
  }

  if (authentication.phase === "anonymous") {
    return (
      <main>
        <h1>Sunshine Manager</h1>
        <form onSubmit={submit}>
          <input
            type="text"
            name="username"
            aria-label="管理员用户名"
            autoComplete="username"
            maxLength={64}
            required
            spellCheck={false}
            value={username}
            onChange={(event) => setUsername(event.target.value)}
          />
          <input
            type="password"
            name="password"
            aria-label="管理员密码"
            autoComplete="current-password"
            required
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
      <p>{authentication.session.username}</p>
      <button onClick={leave}>退出</button>
      {error && <p>{error}</p>}
      <pre>{JSON.stringify(hosts, null, 2)}</pre>
    </main>
  );
}
