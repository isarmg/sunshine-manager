# Sunshine Agent 管理协议与验收记录

当前代码已有 Manager 设备注册/撤销、WSS 接入、任务投递与结果审计，以及 Agent 的可运行入口、受保护配置和双平台系统服务。
客户端独立交付流程见 [Agent 说明](../agent/README.md)与 [0.1.0-rc.1 候选说明](releases/agent-0.1.0-rc.1.md)。
本页保留早期执行内核的设计及历史测试记录；其历史待办不能当作当前代码缺失清单，也不能反过来当作完整验收声明。
数据转换、备份恢复、升级仍属于 `sarmg-upgrade`，不修改旧实例来绕过身份验证。

## 明确的适配目标

- Agent 目标系统：Windows x86_64、Linux x86_64；不继承 Foundation Server 的 Linux-only 构建门禁。
- Sunshine：官方稳定版 **v2026.516.143833**，2026-09-05 查询官方 `releases/latest` 得到此版本。
- 协议常量：`sunshine-management/1`；未识别版本必须拒绝，不自动猜测兼容性。
- Agent 与协议代码位于本产品 `agent/`、`protocol/`，没有放入 Foundation，也不另建通用 Agent 平台。
- 依赖 Foundation Agent 的不可变 `0.6.0` / `a5b3158aa62af8189dd909500758cee649898e5c`。
  Linux 日志复用其安全文件系统；HTTPS 复用其有界安全网络能力。不修改已发布版本。

官方依据：

- [固定发布版本](https://github.com/LizardByte/Sunshine/releases/tag/v2026.516.143833)
- [该版本配置 HTTP 实现](https://github.com/LizardByte/Sunshine/blob/v2026.516.143833/src/confighttp.cpp)
- [该版本字段解析](https://github.com/LizardByte/Sunshine/blob/v2026.516.143833/src/config.cpp)

这个版本提供 HTTP Basic 认证。本机非浏览器请求不带 Origin/Referer，符合该版本明确支持的 CSRF 分支；不是禁用 Manager 的浏览器 CSRF 防护。配置 GET 返回状态、平台、版本和文件配置，POST 重写文件。配置读回只证明文件内容，不证明运行时已经使用它。

## 协议与本机执行约束（已实现）

`Task` 包含协议版本、Foundation 操作 ID、Manager/设备/安装身份绑定、操作权限和业务指令。
操作 ID 使用 `op_` 加规范小写非空 UUID。内容指纹覆盖绑定、权限及完整指令，与 JSON 对象顺序无关。
`DeliveryMode` 在指纹外，仅区分正常投递和只核对模式，不允许它改变业务指令。

仅三种指令：

| 指令 | 必需输入 | 执行限制 |
| --- | --- | --- |
| `read_config` | 绑定、读取权限 | 只回传受管字段及完整配置修订，不回传私有配置 |
| `patch_config` | 预期修订、`set`、`remove`、`restart_policy: manual` | 白名单、强类型、长度、取值校验；不自动重启 |
| `restart` | 预期修订、管理员明确确认 | 同时要求 Agent 本机允许重启；不提供脚本或进程透传 |

`Executor::deliver` 是认证后的内部入口，不是公开 RPC；Manager 的 WSS 接入层必须先完成独立设备凭据验证和撤销检查。
枚举里的 `permission` 是经认证 Manager 的业务授权声明，不是设备自行提权或未认证调用的权限凭证。

第一版保守白名单为名称、受限日志级别、QP、HEVC/AV1 模式、线程数、软件编码预设及部分 NVENC 参数，详见 `protocol/src/config.rs`。
不允许应用命令、准备命令、提权、文件路径、网络监听/加密/CSRF 策略、驱动安装、任意 HTTP 和任意下载安装。
受管名称最多 32 个 Unicode 字符。日志级别不开放 debug/verbose 或关闭日志。
产品主动收窄的范围（例如线程数 1–64）不表示 Sunshine 原生只支持该范围。

本机适配器只接受根路径、无用户信息/查询参数的 **loopback IP HTTPS URL**，不接受远程 IP 或可变 DNS。
必须预置可信证书且证书 SAN 匹配该 IP；不自动信任 Sunshine 自签名证书。
请求无代理、不跟随重定向、验证证书、限制请求/响应体和时间；错误只返回固定分类，不输出凭据、上游响应或配置。
凭据暂只由构造函数接收，**本机凭据落盘和安装程序仍未接入，不能据此部署生产 Agent**。

配置变更顺序：校验任务 → 持久化操作身份 → 读取全配置 → 核对修订 → 过滤响应元数据并合并设置/删除 → 再读核对 → 持久化副作用意图 → 提交完整合并配置 → 回读全配置核对 → 持久化结果。
未改动的未知字段和本机高风险字段留在本机并原样保留，不能借远程删除绕过白名单。空值按 Sunshine 写入语义归一化。

修订来自全量配置的规范有序哈希，不只包含受管字段。一个实例只有一条执行通道，并发相同修订只能有一个成功者。
二次检查可减少竞争窗口，**不是 Sunshine 原生原子 CAS**；受管字段由 Manager 统一管理，不允许同时用本机编辑器修改它们。
原始文件注释、字段顺序不属于 Sunshine 配置 API 能保留的内容，不承诺字节级文件保留。

## 去重、崩溃与效果判断（已实现内核）

- Agent 仅持久化指纹、写入目标修订/重启意图及结果，不保存第二套 pending/running 等任务状态。
- 同 ID 不同内容拒绝；有最终结果的重复投递直接返回，已持久化意图的操作不再执行副作用。
- 写入后回执丢失、持久化失败或崩溃，优先读回实际完整修订；一致才报告已保存。
- 重启回执丢失不能靠文件相同或 API 在线推断重启成功，返回 `unknown`，不会再次重启。
- `inspect_only` 不会写配置或重启，没有执行记录也不猜测执行结果。
- 保存成功为 `awaiting_restart`；重启得到响应且重新读回一致仍为 `pending_verification`。
  当前接口不能证明运行时生效，因此协议不提供可被错误填写的“已验证生效”值。
- Linux 日志目录 0700、记录 0600，持有排他锁，复用 Foundation 原子写入和 fsync。
  最多 4096 条、每条 16 KiB，不自动淘汰操作 ID；满容量拒绝新任务。持久化失败后日志实例关闭写入能力，要求重新打开并核验。

Manager 将来必须将执行观察映射到现有 Foundation 操作结果和审计事务。Agent 的 `Unknown` **不是**新增状态机。
执行结果不确定时沿用 Foundation 的 unknown/管理员核对规则，不能静默标记成功或伪造人工确认。

## WSS 客户端传输（已实现模块，Manager 接入待完成）

固定路径 `/sunshine-agent/v1/connect`，仅接受 `wss://`，禁止 URL 内凭据、查询参数和任意路径。
WebSocket 子协议为合法 HTTP token `sunshine-management.v1`，消息内协议仍为 `sunshine-management/1`。
握手使用敏感 Authorization 头和独立 256-bit 设备凭据，校验证书链、主机名及显式预置 CA；不跟随重定向。
单帧/消息上限 64 KiB、写缓冲上限 128 KiB、握手/写入超时 10 秒；同一会话最多一个执行任务。
15 秒心跳和 Ping，45 秒没有对端活动则重连；重连复用 Foundation 的 1–60 秒随机退避。
401/403 或认证通道中的 `revoked` 为终止条件，不无限重试已撤销凭据。
任务执行与心跳独立调度；会话中断会取消本地执行 future，已写入意图的操作仍只能核对，不能盲目重试。
健康观察超过 30 秒即失效，心跳将 Sunshine 可达性报告为未知，不将 WSS 在线误报成 Sunshine 在线。
这个模块未包含注册、系统凭据落盘和 Manager 服务端；测试中的 Manager 是独立 TLS 夹具。

## 测试与构建（当前执行内核）

根工作区默认仍只构建 Server。以下命令显式选择 Agent，不运行 Server 构建脚本：

```sh
cargo test --locked -p sunshine-agent -p sunshine-agent-protocol
cargo clippy --locked -p sunshine-agent -p sunshine-agent-protocol --all-targets -- -D warnings
# 需要 openssl、可绑定 loopback 临时端口；CI 必须额外执行，不能只看默认忽略后的结果。
cargo test --locked -p sunshine-agent --test local_https -- --ignored
```

这些测试覆盖协议拒绝、配置保留、并发冲突、重复投递、保存/重启回执丢失、执行后崩溃恢复、日志故障、容量满、错误 CA、禁止重定向及响应大小限制。
HTTP 测试使用临时 CA 和协议夹具，不安装 CA 到系统信任库、不修改真实 Sunshine，不等价于真实版本验收。

### 2026-09-05 本地验证记录

- Linux x86_64 / Rust 1.98.0：27 项默认测试及额外 4 项 HTTPS/WSS 测试通过。
- Windows x86_64 MSVC / 原生 Rust 1.98.0：24 项默认测试及额外 4 项 HTTPS/WSS 测试通过。
  三项 Linux 安全文件日志测试未在 Windows 执行，不计入 Windows 验收。
- 两个平台均通过 `cargo clippy --all-targets -- -D warnings`。
- Windows 链接器的“创建库/对象”提示仍被 Rust 显示为 `linker_messages` 警告，编译返回成功；不宣称全部构建输出零警告。
- 现有 Manager 56 项库测试通过；未重建/重启正在运行的服务，未触碰旧实例数据。
- `cargo fmt --all -- --check`、工作流供应链策略及其拒绝用例通过。
- 已配置固定 Linux runner 与仅面向 Agent 的 `windows-2025` CI 作业；尚未推送或运行 GitHub Actions，不宣称远端 CI 通过。

Windows 命令需在原生 PowerShell 中执行，使用 Rust 1.98.0 和 MSVC 工具链，
把已安装 OpenSSL 的目录加入当前进程 PATH 后运行相同的 Cargo 测试命令。
Windows 的原生构建产物与 Linux `target/` 必须分开；可用 `CARGO_TARGET_DIR` 指定独立目录。

## 历史开发切片待办（保留当时记录，不代表当前实现状态）

- [ ] Agent 可运行入口、受保护凭据配置、一次性注册流程和安装身份持久化。
- [ ] Manager 设备表及注册/绑定/撤销 API；独立凭据仅存哈希，配对码不设有效期、仅能绑定一次且可主动取消。
- [x] Agent WSS 客户端主动连接、心跳、退避重连、能力上报、帧大小与背压限制、撤销停止。
- [ ] Manager WSS 认证接入、能力校验、旧会话隔离和完整断线重连验收。
- [ ] Manager 使用 Foundation 持久化操作的投递、断线 unknown 与结果/审计原子处理。
- [ ] 删除 Manager 直连执行及 Sunshine 密码存储，删除不在 v1 范围的业务入口；不保留旧模式分支。
- [ ] 新数据库身份、空库初始化验收及版本发布契约；已有数据库交由升级职责处理，不直接清空。
- [ ] 前端设备注册、三类独立状态、配置/差异预览、冲突、重启确认和任务结果。
- [ ] Windows 原生安全目录/ACL/重解析点与持久化处理。Foundation 0.6.0 的 Windows 私有目录问题仍是阻断项，不能用 Linux 测试替代。
- [ ] Windows 与 Linux 安装、自启动、卸载、独立进程存活测试和分平台发布构建。
- [ ] 两个平台上真实 Sunshine v2026.516.143833 验收，含实际重启中断、证书名/有效期错误和凭据撤销。
- [ ] 网页提交 → WSS 可靠送达 → 本机安全执行 → 状态可核验且可审计的端到端验收。
