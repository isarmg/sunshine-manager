import { t } from "../shell/i18n.js";

/** Display only: protocol values in requests, stored tasks and snapshots stay exact. */
const labels: Record<string, readonly [string, string]> = {
  read_config: ["读取配置", "Read configuration"], patch_config: ["保存配置", "Save configuration"], restart: ["重启", "Restart"],
  config_read: ["配置已读取", "Configuration read"], config_saved: ["配置已保存", "Configuration saved"],
  restart_acknowledged: ["已确认重启请求", "Restart request acknowledged"], conflict: ["修订冲突", "Revision conflict"],
  rejected: ["执行被拒绝", "Execution rejected"], unknown: ["结果不确定", "Outcome unknown"],
  invalid_task: ["任务无效", "Invalid task"], binding_mismatch: ["设备绑定不匹配", "Device binding mismatch"],
  permission_denied: ["权限不足", "Permission denied"], operation_id_reused: ["操作标识被不同内容复用", "Operation ID reused with different content"],
  unsupported_version: ["版本不受支持", "Unsupported version"], unsafe_configuration: ["配置不安全", "Unsafe configuration"],
  sunshine_unavailable: ["Sunshine 不可访问", "Sunshine unavailable"], journal_full: ["执行日志已满", "Execution journal full"],
  no_execution_record: ["没有可核对的执行记录", "No execution record"], effect_not_confirmed: ["生效状态未确认", "Effect not confirmed"],
  restart_not_confirmed: ["重启结果未确认", "Restart not confirmed"], persistence_failure: ["执行记录保存失败", "Execution record persistence failed"],
  true: ["启用", "Enabled"], false: ["禁用", "Disabled"], info: ["信息", "Information"], warning: ["警告", "Warning"],
  error: ["错误", "Error"], fatal: ["致命错误", "Fatal"],
  ultrafast: ["极快", "Ultrafast"], superfast: ["超快", "Superfast"], veryfast: ["很快", "Very fast"], faster: ["较快", "Faster"],
  fast: ["快速", "Fast"], medium: ["中等", "Medium"], slow: ["慢速", "Slow"], slower: ["较慢", "Slower"], veryslow: ["很慢", "Very slow"],
};
export function operationLabel(value: string): string {
  const key = value.replace(/^sunshine\./, "");
  const label = labels[key];
  return label ? t(...label) : t("未识别的状态", "Unrecognized state");
}
export function configValueLabel(value: string): string {
  const label = labels[value];
  return label ? t(...label) : value;
}
