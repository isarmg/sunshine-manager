# 09. 部署、安全与生产运维

## 9.1 拓扑

root-owned 不可变 release、专用服务账户、0600 环境、独立 SQLite 和 runtime 锁；应用仅回环监听，可信
HTTPS proxy 暴露浏览器 API。正式 Server/Web 发行树只支持 Linux AMD64；被管理 Sunshine/Moonlight
外部端平台不变。到 Sunshine Host 的网络按最小范围放行。

## 9.2 上线顺序

从干净 annotated tag 构建并保存 checksum；安装到新精确版本目录；验证 ownership/mode/manifest；准备
当前数据库和 key；运行 verify/doctor；启动并检查 readiness、登录、Host 读取与测试 operation。

## 9.3 关键配置

DATABASE_URL、STATIC_DIR、BIND、credential key ID/key、Session TTL/Cookie、cover allowlist/proxy origin
共同定义安全边界。配置变化需评审并留摘要，不记录原始 Secret。

## 9.4 日常监控

观察 readiness、登录限流、Session 数、operation pending age/unknown、per-Host latency、Sunshine 网络错误、
outbox、SQLite/WAL、磁盘/inode、cover 拒绝与一次性代理失败。

## 9.5 当前数据连续性限制

产品没有 backup/restore；`sarmg-upgrade` 提供 Sunshine 0.8.0 当前状态的 keyed backup/verify/restore，
但没有 Sunshine 历史 edge。数据库副本必须与 external key 独立配对保全；新部署可以恢复严格验证的
0.8.0 备份，或创建全新当前数据库并重新登记 Host；
不要对非当前库逐表复制、手改 metadata 或假定 SQLite 文件复制能够恢复。

## 9.6 Key rotation

当前没有 key rotation/re-encryption 实现。若 key 泄露，停止服务，建立全新当前数据库与 key，并重新
登记/轮换所有 Sunshine credential。未来只有升级仓明确实现并测试的全密文转换 edge 才可原地处理；产品
不提供多 key fallback。

## 9.7 故障处置

| 现象 | 处置 |
|---|---|
| unknown 增长 | 暂停相关 Host 写入，核对远端实际状态 |
| 解密失败 | 核对 key ID/来源/权限，不尝试多个 key |
| Schema drift | 停止并保全，不手改 metadata |
| 大量 401 | 检查 Session/代理/时钟，不降低安全项 |
| cover SSRF 告警 | 禁用该 allowlist，隔离 egress，保全审计 |

## 9.8 安全事件

隔离公网与受影响 Host，保全 release SHA、数据库 generation、审计和脱敏日志，撤销管理员 Session 并
轮换管理员密码、Sunshine credential 与 TLS key。credential key 泄露时不能在原库内轮换：按 9.6 建立
全新当前数据库/key。公开报告不包含生产 URL、数据库或 Secret。

## 9.9 回滚

本产品不支持跨版本状态回滚。部署失败时保持服务停止；只有仍与该数据库/key/current identity 精确匹配的
同一不可变发行物可重新启动。不存在已登记升级 edge 时，应建立全新当前部署，不能只替换 binary 或手改库。
