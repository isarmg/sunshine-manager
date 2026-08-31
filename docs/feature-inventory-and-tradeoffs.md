# Sunshine Manager 完整功能与取舍清单

## 0. 开发者决策台账

本表按功能闭包列出当前实现。分类取“核心、保障、可选、建议保留、开发运维”；复杂度包含 API、数据库、worker、Web、测试和发布联动。删除功能必须清除入口、持久状态、后台任务、依赖、测试和文档，隐藏按钮不算删除。

| ID | 功能/特性与当前实现 | 实现/主要依赖 | 分类 | 复杂度 | 删除后的确定后果 | 最低验证 |
|---|---|---|---|---|---|---|
| SUN-001 | Sunshine Host CRUD 与 revision | routes/model/db/Web | 核心 | 高 | 无法登记或维护上游主机，控制面失去对象 | DTO、并发 revision、审计 |
| SUN-002 | Host credential 受认证加密 | external key、SecretBox、hosts 表 | 保障 | 高 | 明文落库扩大泄露；删 key 支持则已有 Host 不可用 | 错 key/篡改/全记录认证 |
| SUN-003 | 上游只允许 HTTPS | `web_url`、client 构造 | 保障 | 低 | 放宽后凭据和控制请求可被窃听或篡改 | HTTP endpoint 拒绝 |
| SUN-004 | 强制验证 TLS 链、有效期和主机名 | 单一 reqwest client、平台 trust store | 保障 | 中 | 删除会允许 MITM；不存在关闭开关或兼容字段 | 自签/错主机/过期拒绝，可信 CA 成功 |
| SUN-005 | 禁止上游 redirect | reqwest policy、所有 Sunshine 请求 | 保障 | 中 | 凭据或带权请求可能被转发到未知 origin | 30x 响应拒绝 |
| SUN-006 | 有界连接、请求、正文和 JSON | client、日志/封面专用上限 | 保障 | 中 | 慢或恶意 Host 可耗尽 worker/内存 | timeout、超大 body、错 MIME/JSON |
| SUN-007 | Applications 查询与 CRUD | client API、routes、operations、Web | 核心 | 高 | 不能管理 Sunshine 应用定义 | 列表、保存、删除、unknown |
| SUN-008 | Clients 列表、启停与 unpair | client API、operations、Web | 核心 | 高 | 不能管理 Moonlight 配对客户端 | 单个/全部 unpair、权限、断线 |
| SUN-009 | PIN/name pairing | client API、operation、审计 | 建议保留 | 中 | 新 Moonlight 客户端需直接操作 Sunshine | 幂等、错误 PIN、超时 |
| SUN-010 | 配置读取与受限保存 | typed request、严格响应、operation | 建议保留 | 高 | 只能管理应用/客户端，不能统一配置 | 未知字段、revision、远端拒绝 |
| SUN-011 | restart/reset display/close app 等系统动作 | durable operation、per-Host worker | 可选 | 高 | 仍可管理静态配置，但少了运行控制 | 响应丢失→unknown、权限 |
| SUN-012 | Durable Operation 完整状态机 | operations 表、worker、API/Web | 核心 | 高 | 非事务上游的失败语义会被错误简化为同步成功/失败 | 状态转换、崩溃、恢复 |
| SUN-013 | per-Host 串行、跨 Host 并发 | claim 查询、Host lock/fencing | 保障 | 高 | 同一 Host 写入可能乱序；全局串行则吞吐下降 | 同/异 Host 并发测试 |
| SUN-014 | Idempotency-Key 与请求绑定 | route、DB 唯一约束、actor/Host/action | 保障 | 高 | 网络重试可能重复不可逆动作 | 同键同体复用、异体冲突 |
| SUN-015 | running 重启转 unknown，禁止盲重试 | startup recovery、operation policy | 保障 | 高 | 可能重复 unpair/restart 等已生效动作 | 各故障点重启测试 |
| SUN-016 | dead-letter 与人工 resolved | operation 管理 API、审计 | 建议保留 | 中 | 长期失败只能留在普通队列，无法有责收口 | resolve 权限、审计、不可改原结果 |
| SUN-017 | requested/completion/resolved audit outbox | DB 事务、outbox dispatcher | 保障 | 高 | 控制操作缺失完整审计或出现业务/审计分裂 | sink 失败与稳定 ID 重投 |
| SUN-018 | 本地管理员和 Argon2 登录 | auth、users、登录预算 | 核心 | 中 | Web/API 无身份边界 | 正误密码、未知账号、参数上限 |
| SUN-019 | 摘要 Session、idle/absolute TTL、撤销 | sessions、Cookie、清理 | 保障 | 高 | 不能安全维持/撤销浏览器登录 | 过期、撤销、Secure Cookie |
| SUN-020 | CSRF + Origin/Host 检查 | middleware、Session CSRF | 保障 | 高 | 浏览器登录态可被跨站利用 | unsafe method 全矩阵 |
| SUN-021 | 登录来源/账户/全局限流 | auth state、Argon2 budget | 保障 | 中 | 暴力破解和 CPU DoS 风险上升 | 并发、窗口恢复、地址规范化 |
| SUN-022 | 外部封面 HTTPS allowlist | cover policy、配置、operation | 可选 | 高 | 删除后只能使用 Sunshine 已有封面；放宽则产生 SSRF | scheme/host/userinfo 拒绝 |
| SUN-023 | DNS 全公网校验、二次解析与 pin | cover proxy、resolver、egress | 保障 | 高 | DNS rebinding 可访问内网/metadata | 私网/混合结果/重绑定测试 |
| SUN-024 | 封面 no-redirect、MIME/8MiB/timeout | cover downloader | 保障 | 中 | 可被跳转、慢流或大正文滥用 | 30x、错 MIME、超限、超时 |
| SUN-025 | 一次性封面代理绑定 Host/operation/peer/30s | in-memory token、internal route | 保障 | 高 | token 可重放、跨 Host 使用或长期泄露 | 二次取用、过期、错 peer/Host |
| SUN-026 | SQLite 当前 Schema 与 metadata identity | schema.sql、`product_metadata`、启动验证 | 保障 | 高 | 错库/漂移库可能被当当前状态运行 | DDL SHA、integrity/FK、未知表 |
| SUN-027 | 单实例运行锁和 maintenance 排他 | runtime lock、database sibling lock | 保障 | 高 | 两个控制面可重复 claim 和写库 | 双启动、doctor/维护冲突 |
| SUN-028 | 严格 `/api/v2`、DTO 未知字段拒绝 | router、serde `deny_unknown_fields` | 保障 | 中 | 拼错字段可能静默丢失，旧 API 兼容负担回归 | 路由与 unknown field 负例 |
| SUN-029 | React/Vite 同源控制台 | `clients/web`、API client | 建议保留 | 中 | 仍可用 API，但失去内置 UI | build、auth、operation 轮询 |
| SUN-030 | `doctor` 只读检查 | DB/key/目录/static contract | 开发运维 | 中 | 部署验收与故障定位变弱 | 健康和各篡改场景 |
| SUN-031 | source-bound 固定 release tree | `scripts/package-release.py`、release manifest | 开发运维 | 高 | 无法证明二进制、Web、service 同提交同版本 | extra/missing/tamper/relocate |
| SUN-032 | CI Rust/Web/SQLite/release 合同 | workflow、unit/integration tests | 开发运维 | 中 | TLS、Schema 和目录回归可能进入发行 | clean checkout 全门禁 |
| SUN-033 | 中文学习、流程、功能和运维文档 | `docs/`、README | 开发运维 | 低 | 边界和 unknown 语义依赖口头知识 | 链接与命令抽查 |
| SUN-034 | 明确不做 Moonlight 媒体、Host OS 任意命令、SSO、多活、旧版兼容 | 路由/依赖中不存在这些能力 | 核心 | 高 | 任一新增都会改变威胁模型、带宽或一致性架构 | 独立设计评审和全链路测试 |

## 1. 功能清单

| 领域 | 当前能力 | 取舍/限制 |
|---|---|---|
| 身份 | 本地管理员、Argon2、Session、CSRF、登录限流 | 不依赖共享 SSO；需单独管理账户 |
| Host | 新增、修改、删除 Sunshine 连接与凭据 | 远端是非事务性不可信 peer |
| 客户端 | 查看和管理 Sunshine 客户端 | 能力受 Sunshine API 支持范围约束 |
| 应用 | 查询、创建、修改、删除应用配置 | mutation 全部异步，不同步伪装成功 |
| Operation | durable、幂等、per-Host 串行、恢复、状态查询 | `unknown` 需要人工核对 |
| 审计 | requested/completion 与 durable outbox | 不记录 Secret 或远端正文 |
| 封面 | HTTPS allowlist、DNS 公网校验、pin、一次性代理 | 需要 Sunshine 主机可达内部 HTTPS origin |
| 数据 | 当前 SQLite、字段密文、运行锁、doctor | 单数据库单活，不是集群数据库 |
| Web | 独立 React/Vite 控制台 | 不提供插件系统 |
| 发布 | 固定目录、source-bound、全树 manifest | 同版本不覆盖，无 mutable alias |

## 2. 架构取舍

- 使用 durable operation 而不是同步远端写入，准确表达网络不确定性；UI 和调用方必须处理异步状态。
- 每 Host 串行避免同一 Sunshine 的写入乱序，不同 Host 并发维持吞吐；单 Host 慢操作会形成局部队列。
- SQLite 降低独立部署成本，依靠单实例锁保证执行互斥；不支持多个进程共享同一库做 active-active。
- SecretBox/AES-GCM 风格的受认证密文保护数据库泄漏场景；external key 丢失无法恢复，因此密钥备份是
  业务连续性的硬要求。
- 封面由一次性反向代理隔离原始 URL，降低 SSRF；需要额外 DNS/egress 和代理拓扑配置。

## 3. 安全边界

默认 loopback listener，生产由可信 HTTPS proxy 暴露并使用 Secure Cookie。所有 Sunshine 主机连接
固定使用 HTTPS，并由平台信任库强制验证证书链、有效期与主机名；API、数据库和 Web 没有关闭校验的
字段。外部 URL allowlist 不是 Sunshine egress firewall 的替代。
数据库、credential key、环境文件和 release 分别以最小权限保护。

## 4. 当前版本边界

- 只注册 `/api/v2/auth/*` 与 `/api/v2/sunshine/*`。
- 只接受 `sunshine-manager 0.7.0`、Schema revision 1、SHA-256
  `a8e2fe3c3a9a59a9a36979bcef3628299832d02078e421953ab78e0c0900d5a7`。
- 只接受配置的当前 credential key ID/key；没有 previous-key keyring。
- 不读取 migration ledger、旧库或未知发行 manifest。
- 不包含 backup、restore、migration、key rotation 或 re-encryption 实现。

## 5. 明确不提供

不传输 Moonlight 媒体、不管理 Host OS、不远程执行任意命令、不自动重试 unknown、不允许任意封面 URL、
不提供容器/共享中央运行时，也不为旧 API、旧 Schema 或旧 Secret 添加兼容逻辑。

## 6. Host 连接能力详解

| 项目 | 当前行为 | 安全/可靠性理由 | 操作者责任 |
|---|---|---|---|
| Endpoint | `https://<host>:<web_port>` | 不支持明文 Sunshine API | 配置可解析 DNS/IP 与端口 |
| TLS | 总是验证链、有效期和主机名 | 不允许中间人窃取 credential/修改操作 | 私有 CA 先纳入平台信任库 |
| Credential | 当前 external key 认证加密 | 数据库泄露不直接暴露密码 | 独立备份 key，限制环境权限 |
| Timeout | 有界连接/总请求 | 慢 Host 不无限占 worker | 监控网络和 Host 延迟 |
| Redirect | 禁止 | 防 credential/请求被转发到未知 origin | 配置最终规范地址 |
| Response | 状态、Content-Type、大小、JSON 形状验证 | Sunshine 是不可信网络 peer | 版本/响应异常先隔离 Host |

创建/修改 Host 的 API、数据库和 Web 均没有 TLS 校验开关，所有未知字段都会被严格拒绝。即使在开发
模式，Sunshine 上游 TLS 验证也不会放宽；开发模式只影响本地浏览器 Cookie/监听边界。

## 7. 远端操作目录

| 领域 | 读取能力 | Mutation | 幂等/不确定性 |
|---|---|---|---|
| Applications | 列表、配置读取 | 保存、删除、关闭当前应用 | 全部持久 operation |
| Clients | 列表与 enabled 状态 | unpair、unpair all、update | 远端断线可能 unknown |
| Pairing | 读取必要配置 | PIN/name 配对 | 审计且按 Host 串行 |
| Configuration | 当前配置/locale | 保存受限对象 | 严格 body 与 operation |
| System | 状态/日志 | restart、reset display | 重启响应丢失尤其可能 unknown |
| Covers | 当前 cover 读取 | 安全下载后一次性代理上传 | 绑定 operation/Host/peer/时限 |

具体字段和动作以当前 Rust enum、Sunshine client 与测试为准。没有进入 route、operation 和测试的 Sunshine
API 不属于支持范围。

## 8. Operation 状态与调用方行为

| 状态 | 精确含义 | UI/自动化应该做什么 |
|---|---|---|
| pending | 事务已保存意图，等待 worker | 轮询原 operation，不生成新 key |
| running | 已 claim 并可能开始上游调用 | 保持等待，不并发修改同 Host |
| succeeded | 可证明上游成功 | 刷新 Host actual state |
| failed | 可证明业务拒绝/未成功 | 展示安全 error code，修正后新意图 |
| unknown | 上游副作用无法证明 | 人工查询 Sunshine，禁止盲重试 |
| dead_letter/resolved | 超出自动尝试或经人工确认 | 保留审计与确认者，不伪造原结果 |

相同 actor/Host/action/Idempotency-Key 与相同请求返回原 operation；不同请求复用键返回冲突。资源 revision
防止基于过期 UI 覆盖，和幂等键解决的是两个问题。

## 9. 认证和审计功能

| 能力 | 当前保证 | 限制 |
|---|---|---|
| 登录 | Argon2、未知账户等成本、来源/账户/全局预算 | 本地账户，不共享 SSO |
| Session | 随机 Token 摘要、idle/absolute TTL、撤销 | 不跨项目/跨部署共享 |
| CSRF | unsafe method 需绑定 Token + Origin/Host | 可信代理必须保持正确 Host |
| 审计 | requested/completion/resolved durable outbox | 不记录 Secret、encrypted request 或上游正文 |
| 管理维护 | maintenance exclusive | 运行实例存在时拒绝，不在线改库 |

## 10. 封面 SSRF 防护矩阵

| 阶段 | 校验 | 防御目标 |
|---|---|---|
| URL 解析 | HTTPS、规范结构、无 credential | scheme/解析歧义 |
| Allowlist | DNS hostname 精确匹配 | 任意站点访问 |
| 初次 DNS | 所有地址必须公网 | 内网/metadata 访问 |
| 执行 DNS | 重新解析并固定完整集合 | DNS rebinding |
| HTTP | no redirect、timeout、<=8 MiB、图片 MIME | 跳转、慢响应、内存耗尽 |
| 内部交付 | 30 秒、一次性、Host/operation/peer 绑定 | Token 重放和跨 Host 取用 |

应用层检查需与系统 egress firewall 配合；allowlist 为空表示外部封面能力关闭，而不是允许全部。

## 11. 数据与密钥合同

| 数据 | 存储 | 验证/备份 |
|---|---|---|
| Host metadata | SQLite `hosts` | 当前 Schema、普通字段边界 |
| Host credential | 当前 envelope ciphertext | external key 实际认证全部记录 |
| Operation request | encrypted ciphertext | pending/running/终态不变量 |
| Session | Token/CSRF 摘要 | TTL、版本、撤销 |
| Audit/outbox | SQLite | 与业务事务一致、可幂等投递 |
| External key | 数据库外受保护配置 | raw bytes 不进 backup/manifest |

当前数据库 SHA 为上文列出的 code-owned 值。恢复必须同时具备数据库与正确 external key；只改 metadata
或 key ID 不能让错误状态变合法。

## 12. 容量与过载

登录 body/Argon2、Sunshine JSON/日志/封面、Host/客户端/应用集合、operation worker/per-Host queue、outbox、
SQLite/WAL、HTTP 超时都有当前边界。增加容量前应先判断是合法规模还是故障积压，不能通过无界配置隐藏
slow Host 或未知操作。

## 13. 故障语义

| 故障 | 当前结果 | 禁止的“修复” |
|---|---|---|
| TLS 证书无效 | 连接失败，operation 按调用阶段分类 | 关闭验证 |
| 请求后断线 | 可能 unknown | 新 key 自动再发 |
| 运行中进程崩溃 | running 在重启后 unknown | 假定 failed |
| external key 错误 | 启动/doctor 失败 | previous-key fallback |
| Schema/manifest drift | 启动前拒绝 | 手改 SHA/忽略额外文件 |
| 封面 DNS 变化 | 下载拒绝 | 只信首次解析 |
| Outbox 投递失败 | 业务状态保留，按稳定 ID 重投 | 丢弃审计 |

## 14. 候选需求取舍

| 候选 | 当前决定 | 理由 |
|---|---|---|
| 关闭上游 TLS | 删除且禁止 | 会暴露 Sunshine 密码和远端控制 |
| 自动重试 unknown | 不提供 | 可能重复不可逆副作用 |
| 共享 SSO/中央服务 | 不提供 | 扩大在线故障域和跨项目耦合 |
| Host OS 远程管理 | 不提供 | 超出 Sunshine API 控制面威胁模型 |
| Moonlight 媒体代理 | 不提供 | 与管理 API 的带宽/协议/可靠性完全不同 |
| 任意封面 URL/redirect | 不提供 | SSRF 和 credential 转发风险 |
| 多实例 active-active | 不提供 | SQLite、per-Host 串行和 operation claim 为单控制面设计 |

## 15. 功能完成定义

新能力必须具备严格 API、Session/CSRF/RBAC、durable operation 与幂等、Sunshine 响应边界、Secret 加密、
容量/超时、审计 outbox、重启/unknown、Schema/升级合同、Web 行为、release 篡改测试和中文运维。直接在
handler 中调用一次 Sunshine 并返回 200 不构成完整功能。
