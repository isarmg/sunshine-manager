# 09. 部署、安全与生产运维

## 9.1 拓扑

root-owned 不可变 release、专用服务账户、0600 环境、独立 SQLite 和 runtime 锁；应用仅回环监听，可信
HTTPS proxy 暴露浏览器 API。到 Sunshine Host 的网络按最小范围放行。

## 9.2 上线顺序

从干净 annotated tag 构建并保存 checksum；安装到新精确版本目录；验证 ownership/mode/manifest；准备
当前数据库和 key；运行 verify/doctor；启动并检查 readiness、登录、Host 读取与测试 operation。

## 9.3 关键配置

DATABASE_URL、STATIC_DIR、BIND、credential key ID/key、Session TTL/Cookie、cover allowlist/proxy origin
共同定义安全边界。配置变化需评审并留摘要，不记录原始 Secret。

## 9.4 日常监控

观察 readiness、登录限流、Session 数、operation pending age/unknown、per-Host latency、Sunshine 网络错误、
outbox、SQLite/WAL、磁盘/inode、cover 拒绝与一次性代理失败。

## 9.5 备份恢复

使用 `sarmg-upgrade` 对当前 code allowlist SQLite 做一致性备份，并提供 external key 验证全部密文。key
bytes 独立保管。恢复演练必须包含 verify、restore、offline doctor、启动和实际 Host 查询。

## 9.6 Key rotation

停止服务并取得 maintenance exclusive，由升级工具认证旧密文、全量生成新 envelope、验证新 generation
后原子安装。更新受保护 key 配置，再 doctor/start。产品不提供 previous key fallback。

## 9.7 故障处置

| 现象 | 处置 |
|---|---|
| unknown 增长 | 暂停相关 Host 写入，核对远端实际状态 |
| 解密失败 | 核对 key ID/来源/权限，不尝试多个 key |
| Schema drift | 停止并保全，不手改 metadata |
| 大量 401 | 检查 Session/代理/时钟，不降低安全项 |
| cover SSRF 告警 | 禁用该 allowlist，隔离 egress，保全审计 |

## 9.8 安全事件

隔离公网与受影响 Host，保全 release SHA、数据库 generation、审计和脱敏日志，轮换管理员 Session/密码、
credential key、Sunshine credential 与 TLS key。公开报告不包含生产 URL、数据库或 Secret。

## 9.9 回滚

保持服务停止，依据升级工具 journal 明确 commit/rollback，安装与数据库/key 精确匹配的不可变版本，再
doctor 和 smoke。仅回滚 binary 不构成状态回滚。
