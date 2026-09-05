import { createSarmgAdminApplication, errorRequestId, useAdminApplication } from "@sarmg/admin-shell";
import { Button, EmptyState, ErrorState, LoadingState, StatusBadge, Table } from "@sarmg/admin-ui";
import { useEffect, useState } from "react";
import { CURRENT_API_PREFIX, adminApi, isHostInfoArray, type HostInfo } from "./api";

function HostsPage() {
  const { client } = useAdminApplication();
  const [hosts, setHosts] = useState<HostInfo[] | null>(null);
  const [failure, setFailure] = useState<{ requestId?: string } | null>(null);
  const [generation, setGeneration] = useState(0);
  useEffect(() => {
    const controller = new AbortController();
    setHosts(null); setFailure(null);
    void client.request(`${CURRENT_API_PREFIX}/sunshine/hosts`, isHostInfoArray, { signal: controller.signal })
      .then(received => { if (!controller.signal.aborted) setHosts(received); })
      .catch(error => { if (!controller.signal.aborted) setFailure({ requestId: errorRequestId(error) }); });
    return () => controller.abort();
  }, [client, generation]);
  const refresh = () => setGeneration(value => value + 1);
  return <section id="hosts"><h1>Sunshine 主机</h1>
    <Button onClick={refresh} disabled={hosts === null && failure === null}>刷新</Button>
    {failure ? <ErrorState requestId={failure.requestId} onRetry={refresh}>无法加载主机列表</ErrorState>
      : hosts === null ? <LoadingState>正在加载主机…</LoadingState>
      : hosts.length === 0 ? <EmptyState>暂无主机</EmptyState>
      : <Table aria-label="Sunshine 主机"><caption>主机状态</caption>
        <thead><tr><th scope="col">名称</th><th scope="col">地址</th><th scope="col">连接</th></tr></thead>
        <tbody>{hosts.map(host => <tr key={host.id}><th scope="row">{host.name}</th>
          <td>{host.host}:{host.web_port}</td><td><StatusBadge status={
            host.probe_status === "pending" ? "正在检测" : host.connected ? "已连接" : "未连接"
          } /></td></tr>)}</tbody>
      </Table>}
  </section>;
}

export default createSarmgAdminApplication({
  product: { name: "Sunshine Manager", version: "0.8.0" },
  client: adminApi,
  navigation: [{ label: "主机", href: "#hosts" }],
  routes: <HostsPage />,
});
