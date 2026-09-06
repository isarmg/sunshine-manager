import { Button, EmptyState, Table } from "@sarmg/admin-ui";
import type { DeviceInfo } from "./api";

const configurationStates: Record<string, string> = {
  unknown: "尚未核对",
  awaiting_restart: "已保存，等待重启",
  pending_verification: "运行时生效待验证",
  drift_detected: "配置存在差异",
};
function lastSeen(micros: number | null) {
  if (micros === null) return "尚未连接";
  const date = new Date(micros / 1000);
  return Number.isFinite(date.getTime()) ? date.toLocaleString() : "未知";
}
export function DeviceInstances({ devices, select }: { devices: DeviceInfo[]; select(id: string): void }) {
  if (!devices.length) return <EmptyState>暂无实例，请新建并注册 Agent。</EmptyState>;
  return <Table aria-label="Sunshine 实例列表"><thead><tr>
    <th scope="col">实例名称</th><th scope="col">注册状态</th><th scope="col">Agent 状态</th>
    <th scope="col">Sunshine 接口</th><th scope="col">配置状态</th><th scope="col">操作系统</th><th scope="col">最近连接</th>
  </tr></thead><tbody>{devices.map(device => <tr key={device.id}>
    <th scope="row"><Button aria-label={`选择实例 ${device.name}`} onClick={() => select(device.id)}>{device.name}</Button></th>
    <td>{device.revoked ? "凭据已撤销" : device.registered ? "已注册" : device.pairing_pending ? "等待配对" : "配对已取消"}</td>
    <td>{device.agent_online ? "在线" : "离线"}</td>
    <td>{device.sunshine_reachable === null ? "未知" : device.sunshine_reachable ? "可访问" : "不可访问"}</td>
    <td>{configurationStates[device.configuration_state] ?? "待核对"}</td>
    <td>{device.capabilities?.os ?? "尚未上报"}</td><td>{lastSeen(device.last_seen_at_micros)}</td>
  </tr>)}</tbody></Table>;
}
