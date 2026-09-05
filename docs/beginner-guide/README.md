# Sunshine Manager 初学者学习指南

本手册以十章建立完整心智模型：先区分 Manager 与 Sunshine，再理解认证、持久操作、封面安全、测试和
运维。后面的单页内容是快速导读，专题章节提供变更与故障处置所需的细节。

1. [项目全景与版本边界](01-project-overview.md)
2. [开发环境与第一次运行](02-environment-and-first-run.md)
3. [Rust、HTTP 与 Web 基础](03-rust-http-and-web-basics.md)
4. [登录、Session 与请求生命周期](04-authentication-and-request-lifecycle.md)
5. [持久操作、幂等与恢复](05-durable-operations-and-recovery.md)
6. [Sunshine Host、客户端与应用管理](06-host-client-and-application-management.md)
7. [封面代理、SSRF 与当前协议](07-cover-proxy-and-current-contracts.md)
8. [测试、调试与变更方法](08-testing-debugging-and-change-workflow.md)
9. [部署、安全与生产运维](09-deployment-security-and-operations.md)
10. [源码路线、练习与术语表](10-reading-roadmap-and-glossary.md)

以下保留单页速览。

## 1. Sunshine 与 Manager 的关系

Sunshine 是运行在被管理主机上的游戏串流服务；Sunshine Manager 是另一个控制面，通过 Sunshine API
管理 Host、客户端和应用。Manager 不传输游戏画面，也不替代 Sunshine 本身。远端 Sunshine 应视为不
可信网络 peer：响应可能超时、错误或在断线前已经执行操作。

## 2. 目录与模块

```text
sarmg-foundation-server                 管理员 Session、CSRF、登录限流与认证路由
src/client.rs                     Sunshine HTTP client
src/operations.rs                 持久异步操作、幂等和恢复
src/cover_policy.rs               外部封面 URL 准入
src/cover_proxy.rs                一次性内部封面代理
src/db.rs / database_schema.rs    当前 SQLite 与 doctor
src/release_*.rs                  binary/release manifest 合同
clients/web/                      当前 React/Vite 管理控制台
config/                           源码内运行配置模板
deploy/                           正式服务模板
```

## 3. 开发准备

需要 Rust `1.98.0` 与 Web lockfile 对应的 Node/npm：

```bash
cargo +1.98.0 check --locked --target x86_64-unknown-linux-gnu --all-targets
cd clients/web && npm ci && npm run build
```

开发运行至少设置 SQLite、Web dist、管理员密码和 32 字节 Base64 credential key，并显式开启回环
开发配置：

```bash
cargo run -- serve
```

正式 source-bound binary 只能执行 `serve-release --root ...`。

## 4. 浏览器身份

登录使用本地管理员账户和 Argon2。成功后服务端创建随机 Session 与 CSRF Token，只在 SQLite 保存其
SHA-256 摘要。写请求必须同时满足有效 Cookie、Session 绑定的 `X-CSRF-Token` 和匹配的 Origin/Host。
登录请求受 body 大小、来源、账户与进程级 Argon2 并发/超时限制；未知用户也执行同参数 Hash 工作。管理
身份只有 Foundation 定义的 `admin`，数据库没有角色列，也没有 viewer/operator 授权分支。

当前内置 Web 的产品范围是管理员登录、Session 恢复/退出和 Host 只读概览。Host CRUD、客户端、应用、
配置、封面与 operation 管理由同源 `/api/v2` 提供，但尚未在内置 Web 中实现交互页面；文档不会把 Server
route 错写成已经存在的按钮或表单。

## 5. 主机凭据

Sunshine Host 密码及未完成操作请求使用当前 `SUNSHINE_MANAGER_CREDENTIAL_KEY` 加密。数据库只保存
密文和 key ID。AES-256-GCM 的 AAD 还认证数据用途和记录身份：Host password 绑定 Host ID/`secret`
字段域，operation request 绑定 operation ID/action/`request_ciphertext` 字段域。因此不能把一行的合法密文
复制到另一行或另一字段；即使 key、nonce/ciphertext/tag 和 `sunshine:sgev1:` 前缀本身均合法也会认证失败。
运行时只接受一个当前 key，不尝试其他 key，也不尝试空 AAD。产品运行时没有换 key/重新加密或恢复命令；未来
只有 `sarmg-upgrade` 明确登记的具体 edge 才能承担转换。数据库副本没有对应 external key 时不可使用，
数据库与 key 可以作为一个安全单元交给 `sarmg-upgrade` 的 Sunshine 0.8.0 current-state 命令备份和恢复；
不存在从旧版本转换的兼容路径。

Operation 的 request fingerprint 和 Idempotency-Key 数据库查找值也不能使用裸 SHA-256：同一 master key
经 HKDF-SHA-256 用两个不同 info 派生两把 32-byte key，再分别计算 HMAC-SHA-256。这样数据库副本不能用
已知 JSON 模板离线枚举低熵 PIN，也不能用字典直接识别幂等键；两个域即使输入字节相同，结果也不同。

## 6. 为什么远端写入是异步操作

远端调用可能在 Sunshine 已执行后才超时，因此同步 HTTP 无法可靠断言“失败等于未执行”。Manager
先持久化 operation 与 audit，再由后台 worker 按 Host 串行执行。调用者提供严格 `Idempotency-Key`；
同一 actor/host/action/key 的同请求返回原 operation，不同请求复用 key 返回 409。

状态包括 `pending`、`running`、`succeeded`、`failed`、`unknown`。重启把中断的 running 标为 unknown，
操作者应核对远端状态，不应使用新 key 盲目重放。

## 7. 封面 URL 安全

应用封面可能来自外部 HTTPS URL。Manager 只接受 DNS 名称精确位于 allowlist 且所有解析地址均为公网
的 URL；执行前重新解析一次、固定完整地址集合、禁用 redirect、限制 8 MiB 和图片媒体类型。Sunshine
收到的不是原 URL，而是绑定 Host/Operation/来源地址且 30 秒有效的一次性内部 URL。

这降低 SSRF 和 DNS rebinding 风险，但不能替代 Sunshine 主机自身的 egress 防火墙。

## 8. SQLite 与单实例

一个数据库只允许一个活跃进程。进程全生命周期持有 instance 排他锁和 maintenance shared；管理员
离线维护需要 maintenance exclusive。Linux 使用 `openat2` 锚定父目录，拒绝 symlink、特殊文件和硬链接
alias。锁协议为 `sarmg-upgrade` 当前备份/恢复提供安全边界；当前没有 Sunshine 历史 upgrade edge。

数据库只在文件不存在时创建当前 Schema。元数据、版本、revision 和重新计算的 DDL SHA-256 必须全
匹配。已有库的 main/WAL/journal 会先复制成稳定私有代际，在副本上由 SQLite 校验；源 SHM 只检查普通
文件与单链接身份，所以非当前库、空文件或漂移会在不改写源数据库及 sidecar 字节的前提下拒绝。

## 9. 修改代码的方法

- API 修改同步 Rust、Web 与 `/api/v2` 测试，不添加平行 alias。
- 远端 mutation 必须进入 durable operation，不能在请求 handler 直接执行后宣称原子。
- Secret 不能进入状态响应、audit、日志或 upstream error body。
- cover 变化必须保留 allowlist、两次解析边界、pin、no redirect、大小与 MIME 限制。
- Schema/key 变化的代际转换加入 `sarmg-upgrade`，产品仅定义新当前格式。
- 发布布局变化同步 manifest、静态资源绑定、重定位和篡改测试。

## 10. 术语

- **Idempotency-Key**：把重试绑定到同一持久操作的调用方键。
- **unknown**：外部副作用是否发生无法证明，不等同于失败。
- **SSRF**：服务端被诱导访问内部/敏感网络资源。
- **outbox**：与业务事务一起写入、可在重启后继续投递的审计事件队列。
- **source-bound**：二进制身份包含构建源码完整 revision。

## 11. 平台边界

正式 Server binary 与随发行树提供的内置 Web 只支持 Linux AMD64
（`x86_64-unknown-linux-gnu`）。受管 Sunshine Host、Moonlight 客户端及其协议是外部端，继续保持原有
平台范围；Node lockfile 中构建工具的可选平台包也不代表本项目发布其他 Server target。
