# 01. 项目全景与版本边界

## 1.1 产品是什么

Sunshine Manager 是独立的 Sunshine 控制面。它保存受管 Host 和凭据，查询客户端/应用，并把所有远端
写入建模为可查询、可恢复的持久 operation。它不传输 Moonlight 视频，也不管理 Host 操作系统。

## 1.2 数据流

```text
Browser -> Manager API -> SQLite operation -> worker -> Sunshine API
                \-> audit outbox
```

浏览器只提交意图；SQLite 是 Manager 权威事实；Sunshine 是非事务性远端 peer。网络断线不能证明远端
没有执行，因此同步“调用失败”不是可靠业务状态。

## 1.3 当前身份

当前 `0.7.0` binary、`/api/v2`、Schema revision 1/固定 SHA、credential envelope、Web fingerprint 和
release manifest 是一个不可拆分身份。产品不读取其他代状态，不注册旧路由，不保留 previous key。

## 1.4 主要模块

| 模块 | 责任 |
|---|---|
| auth/login admission | 管理员、Session、CSRF、Argon2 预算 |
| client | 有界 Sunshine HTTP 调用与错误分类 |
| operations | 幂等意图、per-Host 串行、终态与恢复 |
| cover policy/proxy | HTTPS URL 准入、DNS pin、一次性代理 |
| db/schema | 当前 SQLite、锁、doctor |
| release | source-bound binary 与不可变全树验证 |
| clients/web | Host/client/app/operation 控制台 |

## 1.5 Secret 边界

Sunshine credential 与未完成请求在数据库中使用 external key 加密。数据库只保存密文/key ID，原始 key
来自独立 Secret 管理。浏览器、状态 API、审计和日志都不应得到原始 Secret 或上游错误正文。

## 1.6 架构取舍

- durable operation 换取真实不确定性，UI 必须接受异步。
- per-Host 串行防止写乱序，单个慢 Host 会形成局部队列。
- SQLite 易部署，但不做多进程 active-active。
- 封面代理降低 SSRF，要求额外 DNS、TLS 与 egress 边界。
- 单当前 key 简化安全证明，换 key必须停机外部转换。

## 1.7 不提供的能力

不提供串流转发、主机远程 Shell、自动重试 unknown、任意封面 URL、共享 SSO、旧 API/Schema/Secret
兼容、产品内 backup/restore/migration 或运行时插件。

## 1.8 仓库地图

`src/` 是 Rust 控制面，`clients/web/` 是 React/Vite UI，`scripts/` 与 `deploy/` 定义正式发行，`docs/` 解释当前
合同。源码和 code-owned identity 是最终事实源。

## 1.9 本章检查

确认能解释 Manager 与 Sunshine 的所有权、为何远端写入不是数据库事务、为什么数据库和 external key
必须分别备份、为什么 202 不表示远端已成功。
