import { t, getLocale } from "../shell/i18n.js";
import { Button, EmptyState, Table } from "@sarmg/admin-ui";
import type { DeviceInfo } from "./api";

const configurationStates: Record<string, string> = {
  unknown: t("尚未核对", "Not yet checked"),
  awaiting_restart: t("已保存，等待重启", "Saved; waiting for restart"),
  pending_verification: t("运行时生效待验证", "Runtime effect needs verification"),
  drift_detected: t("配置存在差异", "Configuration differs"),
};
function lastSeen(micros: number | null) {
  if (micros === null) return t("尚未连接", "Never connected");
  const date = new Date(micros / 1000);
  return Number.isFinite(date.getTime()) ? date.toLocaleString(getLocale()) : t("未知", "Unknown");
}
export function DeviceInstances({ devices, select }: { devices: DeviceInfo[]; select(id: string): void }) {
  if (!devices.length) return <EmptyState>{t("暂无实例，请新建并注册 客户端。", "No instances yet. Create an instance and register an agent.")}</EmptyState>;
  return <Table aria-label={t("Sunshine 实例列表", "Sunshine instance list")}><thead><tr>
    <th scope="col">{t("实例名称", "Instance name")}</th><th scope="col">{t("注册状态", "Registration status")}</th><th scope="col">{t("客户端 状态", "Agent status")}</th>
    <th scope="col">{t("Sunshine 接口", "Sunshine API")}</th><th scope="col">{t("配置状态", "Configuration status")}</th><th scope="col">{t("操作系统", "Operating system")}</th><th scope="col">{t("最近连接", "Last connection")}</th>
  </tr></thead><tbody>{devices.map(device => <tr key={device.id}>
    <th scope="row"><Button aria-label={t("选择实例 {0}", "Select instance {0}", [device.name])} onClick={() => select(device.id)}>{device.name}</Button></th>
    <td>{device.revoked ? t("凭据已撤销", "Credentials revoked") : device.registered ? t("已注册", "Registered") : device.pairing_pending ? t("等待配对", "Waiting for pairing") : t("配对已取消", "Pairing cancelled")}</td>
    <td>{device.agent_online ? t("在线", "Online") : t("离线", "Offline")}</td>
    <td>{device.sunshine_reachable === null ? t("未知", "Unknown") : device.sunshine_reachable ? t("可访问", "Reachable") : t("不可访问", "Unreachable")}</td>
    <td>{configurationStates[device.configuration_state] ?? t("待核对", "Needs review")}</td>
    <td>{device.capabilities?.os ?? t("尚未上报", "Not yet reported")}</td><td>{lastSeen(device.last_seen_at_micros)}</td>
  </tr>)}</tbody></Table>;
}
