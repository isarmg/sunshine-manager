# Sunshine Manager 完整功能与取舍清单

## 0. 开发者决策台账

本表按功能闭包列出当前实现。分类取“核心、保障、可选、建议保留、开发运维”；复杂度包含 API、数据库、worker、Web、测试和发布联动。删除功能必须清除入口、持久状态、后台任务、依赖、测试和文档，隐藏按钮不算删除。

| ID | 功能/特性与当前实现 | 实现/主要依赖 | 分类 | 复杂度 | 删除后的确定后果 | 最低验证 |
|---|---|---|---|---|---|---|
| SUN-001 | Sunshine Host 登记、查询、修改和删除 | `src/model.rs`、`src/db.rs`、`src/http.rs` Server routes | 核心 | 高 | 控制面没有可管理的上游对象，其他功能全部失去落点；当前内置 Web 仅展示列表，不提供 CRUD 表单 | API CRUD、404、删除后的 operation 行为 |
| SUN-002 | Host 名称、DNS/IP、端口、用户名和密码长度边界 | `validate_host_request`、`normalize_host` | 保障 | 中 | 非法地址或无界文本会延迟到网络层失败并放大资源消耗 | IPv4/IPv6/DNS、控制字符、零端口、极值 |
| SUN-003 | Host 列表位置和创建/更新时间持久化 | `hosts.position`、微秒时间戳 | 建议保留 | 低 | 列表顺序变为不稳定，排障时难以判断对象变更时间 | 多 Host 顺序、更新不改创建时间 |
| SUN-004 | API 永不序列化 Host 密码，只返回 `password_set` | `Host.password` 的 `skip_serializing`、`HostInfo` | 保障 | 低 | 浏览器响应、开发工具或日志可能暴露上游凭据 | Host JSON 快照、空/已设密码 |
| SUN-005 | Host credential 以 AES-256-GCM 受认证加密，AAD 绑定产品域、Host ID 和 `secret` 字段域 | `SecretBox::encrypt_host_credential`、`SecretBox::decrypt_host_credential`、`hosts.secret` | 保障 | 高 | 删除加密会让数据库副本直接泄露密码；删除 AAD 身份绑定会允许合法密文跨 Host/字段替换 | round-trip、随机 nonce、篡改、跨 Host/跨用途调换、空 AAD 拒绝、明文扫描 |
| SUN-006 | 只接受一个当前 external credential master key；AES 直接使用它，两个 operation HMAC key 由 HKDF 独立分域派生 | `SUNSHINE_MANAGER_CREDENTIAL_KEY[_ID]`、`SecretBox::new`、32-byte key | 保障 | 高 | 删除 key 验证会让错密钥延迟到运行期；复用一把 HMAC key 会破坏域隔离；加入多 key 会形成隐式轮换路径 | Base64/长度/key ID、HKDF info 分离、错 key、全记录认证 |
| SUN-007 | Sunshine 上游地址固定为 HTTPS | `web_url`、`UpstreamClient` | 保障 | 低 | Basic credential 和控制请求可被明文窃听或篡改 | 生成 URL、HTTP 配置不存在 |
| SUN-008 | 强制验证 TLS 链、有效期和主机名 | reqwest 默认 verifier、平台 trust store | 保障 | 中 | 允许中间人接管 Sunshine；项目中没有关闭校验的配置或代码路径 | 自签、错主机、过期拒绝，可信 CA 成功 |
| SUN-009 | 所有上游请求禁止 redirect | reqwest `Policy::none` | 保障 | 中 | 认证头或带权 mutation 可能被导向未知 origin | 301/302/307/308 全部不跟随 |
| SUN-010 | 仅需认证的 Sunshine API 附加 Basic Auth | `UpstreamClient::json`、日志/封面请求 | 保障 | 中 | 漏加会导致功能失败；无差别添加会把凭据发送到不需要认证的端点 | 每条上游 route 的请求头断言 |
| SUN-011 | 上游 connect 3 秒、总请求 15 秒 | reqwest client builder | 保障 | 低 | 断线或慢 Host 可长期占用 worker 与 Host 串行锁 | 连接黑洞、慢正文、超时分类 |
| SUN-012 | JSON/日志最大 4 MiB、封面最大 8 MiB | `read_limited`、Content-Length 与流式累计 | 保障 | 中 | 恶意或异常 Sunshine 可耗尽进程内存 | 预声明超限、分块超限、边界值 |
| SUN-013 | Sunshine JSON/日志成功响应验证状态、UTF-8、MIME 与 JSON | `json_response`、`UpstreamClient::logs` | 保障 | 中 | HTML 错页或畸形正文可能被当业务数据进入调用方 | 非 2xx、错 MIME、非法 UTF-8/JSON；封面读取另见 SUN-029 |
| SUN-014 | Applications/Clients 集合最多 512 项并验证元素形状 | `MAX_ITEMS`、UUID 去重与布尔字段校验 | 保障 | 中 | 无界集合或重复客户端会拖垮 UI 并产生歧义操作 | 513 项、重复/空 UUID、非对象应用 |
| SUN-015 | JSON/日志错误正文只做 4 MiB 有界读取后丢弃，cover 非成功正文不读取；API/日志均不保留内容 | `ensure_status`、`status_error`、`AppError::into_response` | 保障 | 低 | 远端正文可能泄露 Secret、Host 数据或用控制字符污染日志 | 401/403/5xx 含敏感标记，确认响应与捕获日志均不存在标记；API 只给稳定通用 message |
| SUN-016 | 周期探测可达性与连接状态快照 | `probe_loop`、`health` map、HostInfo | 建议保留 | 中 | 管理员只能在执行动作后发现 Host 离线 | 500ms TCP 探测、删除清理、并发读取 |
| SUN-017 | Application 列表读取 | `/apps` route、`UpstreamClient::apps_list` | 核心 | 中 | API 调用方无法看到 Sunshine 可启动应用；当前内置 Web 尚无应用页 | 正常/空/超限/异常响应 |
| SUN-018 | Application 顶层 JSON object（≤256 KiB）保存为 durable operation | `validate_object`、`AppsSave`、`/apps` POST | 核心 | 高 | 无法集中新增或修改 Sunshine 应用；逐字段语义当前委托给 Sunshine，而非 Manager 自建 Schema | 非 object/超限、幂等、成功/unknown |
| SUN-019 | 关闭当前应用和按 index 删除应用 | `AppsClose`、`AppsDelete` | 可选 | 中 | 管理端仍能查看/保存应用，但不能结束会话或删除条目 | index 边界、响应丢失、审计 |
| SUN-020 | Moonlight Client 列表 | `/clients`、`clients_list` | 核心 | 中 | 无法盘点已配对客户端和 enabled 状态 | UUID/布尔/重复/上限 |
| SUN-021 | 单个/全部 Client unpair | `ClientsUnpair`、`ClientsUnpairAll` | 核心 | 高 | 泄露或退役客户端必须到每台 Sunshine 手工解除 | 单个、全部、unknown、safe retry 证据 |
| SUN-022 | 修改 Client enabled 状态 | `ClientsUpdate` | 建议保留 | 中 | 只能删除配对，不能临时停用/恢复 | UUID、布尔值、失败与幂等 |
| SUN-023 | 读取 Sunshine 当前配置 | `/config` GET、`config_get` | 建议保留 | 中 | 无法在修改前确认实际配置 | 大小/MIME/JSON、远端权限 |
| SUN-024 | 保存受限 JSON object 配置 | `ConfigSave`、`validate_object` | 建议保留 | 高 | 只能读配置；放宽为任意 JSON 会削弱边界 | object/数组/大小、幂等、unknown |
| SUN-025 | 读取 locale 和受限文本日志 | `/config/locale`、`/api-logs` | 可选 | 中 | 排障与本地化展示变弱，但主要控制功能仍可用 | locale JSON、日志 MIME/UTF-8/4 MiB |
| SUN-026 | PIN/name 配对 | `Pin` operation、`/pin` | 建议保留 | 中 | 新 Moonlight 客户端必须直接操作 Sunshine | PIN/name 边界、串行、审计 |
| SUN-027 | 远端 Sunshine restart | `Restart` operation | 可选 | 高 | 管理员需登录 Host 执行重启；保留时必须接受响应丢失的 unknown | 202、断线、禁止盲重试 |
| SUN-028 | reset display persistence | `ResetDisplay` operation | 可选 | 高 | 显示设备持久状态需在 Host 本地修复 | 成功、远端拒绝、unknown |
| SUN-029 | 读取当前应用封面并将未知 MIME 降级为 `application/octet-stream` | `/covers/{index}`、`UpstreamClient::cover`、`safe_cover_type` | 可选 | 中 | API 调用方无法预览 Sunshine 封面；当前内置 Web 尚无封面页 | index、8 MiB、已知 MIME 映射、未知 MIME 安全降级、认证 |
| SUN-030 | 外部封面安全下载并让 Sunshine 回取 | `CoverUpload`、cover policy/proxy | 可选 | 高 | 只能使用 Sunshine 现有封面或在 Host 本地上传 | 完整 SSRF、token、operation 流程 |
| SUN-031 | Operation 七状态持久状态机 | `pending/running/succeeded/failed/unknown/dead_letter/resolved` | 核心 | 高 | 非事务远端调用会被错误简化为一次同步成败 | 每条允许/禁止转换与重启恢复 |
| SUN-032 | 保存 mutation 意图与 requested outbox 同事务提交 | `OperationManager::enqueue`、`insert_outbox`、SQLite transaction | 保障 | 高 | 进程崩溃可能出现“执行了但无意图”或“有意图无审计” | 各 commit 故障点、原子性 |
| SUN-033 | Operation 请求用 credential key 加密，AAD 绑定产品域、operation ID、action 和 `request_ciphertext` 字段域；启动/worker 还原严格当前 enum 并核对 action | `SecretBox::encrypt_operation_request`、`SecretBox::decrypt_operation_request`、`request_ciphertext`、`RemoteOperationRequest` | 保障 | 高 | 删除加密会暴露 PIN/配置/封面 URL；删除 AAD 或 enum/action 复验会允许密文跨 operation/action 替换或非当前请求潜伏到 worker | round-trip、随机 nonce、跨 operation/action/用途调换、空 AAD/unknown field/action mismatch、明文扫描 |
| SUN-034 | `Idempotency-Key` 仅接受 1–128 个安全 ASCII 字符 | route header parser | 保障 | 低 | 无界或歧义键会污染索引、日志和代理链 | 空、129、Unicode、非法符号 |
| SUN-035 | 幂等身份绑定 actor/Host/action/key 与请求 fingerprint；request 和 key 分别使用独立 HKDF key 的 HMAC-SHA-256 | `operation_request_fingerprint`、`operation_idempotency_key_hash`、`constant_time_equal_32`、SQLite 唯一约束 | 保障 | 高 | 删除幂等会重复副作用；使用裸 SHA-256 会让数据库泄漏者离线枚举低熵 PIN/键；共用域会允许协议值混淆 | 同值稳定、同键同体/异体、跨 Host/action、跨域不同、换 master key 不同、低熵值不等于裸 SHA-256、constant-time 比较 |
| SUN-036 | Operation 查询/解决均绑定当前管理员 actor | `get_for_actor`、`resolve_for_actor` | 保障 | 中 | 知道 operation ID 的其他主体可能读取或改变处理结论 | 错 actor 返回、ID 边界 |
| SUN-037 | 同一 Host mutation 串行、不同 Host 可并行 | `HostMutationLocks`、弱引用锁注册表 | 保障 | 高 | 同 Host 写入可能乱序；若改成全局锁则任一慢 Host 阻塞全站 | 同/异 Host 并发、锁项回收 |
| SUN-038 | worker 同时活跃 Host 最多 16 个 | `MAX_ACTIVE_HOSTS` | 保障 | 中 | 无界并发会打满连接/数据库；过低会降低多 Host 吞吐 | 17 Host 调度、完成后补位 |
| SUN-039 | 启动时把中断的 `running` 原子转为 `unknown` | `recover_startup`、completion outbox | 保障 | 高 | 重启后可能重复已生效操作或永远卡在 running | 崩溃恢复、outbox、pending 不受影响 |
| SUN-040 | Unknown 和终态永不重排执行 | Foundation transition；仅提供 resolve | 保障 | 高 | 人工重排仍可能重复不可逆副作用 | Unknown 阻塞同 Host；人工确认后再处理新意图 |
| SUN-041 | Foundation 拥有 attempt/max_attempts 和 dead-letter 收口 | `_sarmg_operations`、Foundation transition | 保障 | 中 | 不得在产品内实现第二套状态转移或重放规则 | 有界尝试、终态不可再 claim |
| SUN-042 | 只对 unknown/dead-letter 记录证据化 resolved | `OperationResolution`、resolved outbox | 建议保留 | 中 | 管理员核验 Sunshine 后无法留下有责结论 | confirmed 两值、其他状态冲突、原结果不改写 |
| SUN-043 | 对外 `OperationView` 排除 actor/action/request/上游错误正文 | 专用序列化 DTO | 保障 | 中 | 查询接口会泄露敏感意图、身份或远端诊断 | JSON exact-shape、全终态 |
| SUN-044 | 调度与 outbox 每批最多 128，空闲轮询 250ms | `DISPATCH_BATCH`、`OUTBOX_BATCH`、`IDLE_POLL` | 保障 | 中 | 无界扫描会拖垮 SQLite；过小会增加积压延迟 | 大于一批、通知唤醒、空队列 |
| SUN-045 | requested/completion/resolved 与业务状态同事务写 durable audit outbox | `audit_outbox`、`insert_outbox`、`insert_completion_outbox` | 保障 | 高 | 本地操作证据会丢失或与业务状态分裂 | 事务故障、重启、三类事件 |
| SUN-046 | outbox 用稳定事件 ID 幂等物化到本地 `audit_logs` | `deliver_outbox`、`audit_logs_outbox_id_idx`、delivered 标记 | 保障 | 高 | 进程/数据库故障后可能漏记或重复本地审计；当前实现不是外部 sink/exporter | 物化前/事务中故障、同 ID 重放、后台循环续处理 |
| SUN-047 | 管理 Session 精确为 `{authenticated,user_id,username,role,csrf_token}` 且 role 只有 `admin` | Foundation `AdministratorSession`、`_sarmg_administrators` 无 role 列 | 核心 | 高 | 引入 viewer/operator 或增删 Session 字段会扩大授权矩阵并使各项目再次分叉 | 五字段 exact-shape、`authenticated=true`/`role=admin`、Schema 列审计、无权限分支 |
| SUN-048 | 管理员 username 使用 Foundation 唯一规范化和 canonical 规则 | `sarmg-admin-core`、`_sarmg_administrators.username` CHECK/UNIQUE | 保障 | 中 | 大小写/空白别名会破坏唯一性与账户限流；放宽字符会使产品合同分叉 | candidate 1–64 printable ASCII；trim/lower；canonical 3–64、`[a-z0-9._-]`、首尾字母数字；`@`/Unicode/重复拒绝；存量启动检查 |
| SUN-049 | 密码只接受 Foundation 当前 Argon2id 参数和长度 | `sarmg-admin-auth`、bootstrap/login | 保障 | 高 | 弱参数或多验证器会降低抗破解性并积累密码兼容代码 | PHC 参数、上下界、错密码、非当前 hash |
| SUN-050 | 未知账户仍走有成本的密码校验路径 | login handler、固定策略 dummy hash | 保障 | 中 | 响应时间差可用于枚举管理员用户名 | 已知/未知账户耗时和响应等价 |
| SUN-051 | 登录 transport peer 20 次/账户 10 次/5 分钟，不信任 forwarded header | `ConnectInfo`、`LoginAdmission` | 保障 | 中 | 暴力破解可绕过单维限流；经反向代理时所有浏览器共享一个 Server 来源 bucket，代理还需独立限流 | 两个维度、窗口恢复、forwarded spoof 不改变来源、`Retry-After` |
| SUN-052 | 来源和账户两个登录 bucket map 各自最多 4096 项并清除过期/最老项 | `MAX_BUCKETS`、`prune`/`record` | 保障 | 中 | 随机来源/账户攻击可让内存持续增长；总上界是两个 map 各 4096 而非共享 4096 | 两个维度分别超量、时间推进、合法账户继续工作 |
| SUN-053 | Argon2 同时最多 2 个，等待最多 2 秒 | semaphore、timeout | 保障 | 中 | 密码哈希可耗尽异步 worker 和 CPU | 三个并发、超时 429、permit 释放 |
| SUN-054 | Session/CSRF Token 使用 CSPRNG，库内仅保存 SHA-256 摘要 | Foundation `AdministratorService`、`_sarmg_admin_sessions` | 保障 | 高 | 数据库泄漏即可直接接管浏览器会话 | 随机性、摘要长度、明文扫描 |
| SUN-055 | Session 同时执行 idle 与 absolute TTL | `authenticate_session`、配置 TTL | 保障 | 高 | 失窃 token 可无限使用，或合法活动错误越过绝对期限 | idle 刷新、absolute 截断、边界过期 |
| SUN-056 | 正式 Cookie 固定 `__Host-`、HttpOnly、Secure、SameSite=Strict、Path=/ | Foundation `session_set_cookie` | 保障 | 中 | JS、明文链路或跨站请求更容易窃取/滥用 token | production/dev 两模式、Set-Cookie exact 属性 |
| SUN-057 | Session 恢复时轮换 CSRF secret 并替换服务端摘要 | `/auth/session`、`rotate_session_csrf` | 保障 | 高 | 长期复用 CSRF secret 扩大泄露窗口；旧 token 仍有效会削弱轮换 | 新 token 成功、被替换 token 拒绝、并发恢复 |
| SUN-058 | CSRF 只在响应 JSON/前端内存中流转，不设置可读 Cookie | auth handlers、`@sarmg/admin-web` | 保障 | 中 | 可读 Cookie 增加浏览器暴露面，并复制两套 token 来源 | Cookie 集合、刷新/重载、DOM/存储扫描 |
| SUN-059 | unsafe method 同时验证 CSRF 摘要与 Foundation Origin/Host 规则 | protected middleware、`sarmg-admin-auth` | 保障 | 高 | 登录 Cookie 可被跨站表单或错误代理来源利用 | POST/PATCH/DELETE、缺失/重复/错 Origin/Host |
| SUN-060 | 只接受一个形状正确的 Session Cookie | cookie parser、Foundation token shape | 保障 | 中 | 重复 Cookie、截断 token 或歧义解析可能绕过认证 | 多 header、同 header 重复、非法字符/长度 |
| SUN-061 | Logout 服务端写入 `revoked_at_micros` 并过期 Cookie | `/auth/logout`、`revoke_session` | 保障 | 中 | 浏览器删 Cookie 后泄露 token 仍可复用 | 当前 token 失效、重复退出、Cookie 属性 |
| SUN-062 | 登录/恢复/退出成功与所有 `AppError` 响应固定 `no-store, private, max-age=0` | auth handlers、`AppError::into_response` | 保障 | 低 | 浏览器或代理可能缓存身份和应用错误正文；Axum extractor/layer rejection 仍由框架生成，不在此保证内 | 成功、401/429/5xx header，并单测 extractor rejection 的实际形状 |
| SUN-063 | 除登录与内部封面外所有业务 API 必须管理员 Session | router protected layer | 核心 | 高 | Host 凭据和远端控制会变成未认证能力 | 每条 route 匿名/有效/撤销 Session 矩阵 |
| SUN-064 | public body 16 KiB、protected body 1 MiB | Axum `DefaultBodyLimit` | 保障 | 低 | 巨型请求会在解析前占用内存与 CPU | Content-Length/流式超限、边界值 |
| SUN-065 | 只注册当前 `/api/v2` 路径，不提供 alias | router namespace | 核心 | 中 | 平行路径会复制认证、限流和维护测试面 | 当前正例、其他版本/尾斜线/近似路径 404 |
| SUN-066 | 所有请求 DTO 拒绝未知字段 | serde `deny_unknown_fields` | 保障 | 中 | 拼写错误会静默丢失并造成“请求成功但未生效” | 每种 DTO 增加 unknown 字段 |
| SUN-067 | 应用层错误统一为 Foundation `{code,message,retryable,request_id?,details?}` | `sarmg-error`、`AppError` | 保障 | 中 | Web/自动化需维护产品专属分支，内部错误易意外外泄；JSON/Path/body-limit 等框架 rejection 目前不是 `AppError` | 各 AppError 状态码/代码/retryable/exact-shape；单列 extractor rejection |
| SUN-068 | 429 返回有界 `Retry-After`，5xx 不返回内部诊断 | error mapping | 保障 | 低 | 客户端会立即重试造成风暴，或泄漏路径/SQL/Secret | 429、DB、Crypto、Internal |
| SUN-069 | 数据库 URL 只允许 SQLite | runtime config | 核心 | 低 | 接入其他驱动会破坏当前事务、锁和 Schema 假设 | 非 SQLite URL 启动拒绝 |
| SUN-070 | 当前 Schema identity 绑定 application/version/revision/SHA-256 | `sarmg-schema-identity`、`product_metadata` | 保障 | 高 | 错产品、错 DDL 或手改 metadata 的库可能被当当前库 | 四字段逐项漂移、现场 DDL 重算 |
| SUN-071 | 启动验证完整性、外键、全部 Host 当前字段/AAD 密文、全部 operation AAD 密文/严格 enum/action/当前 keyed fingerprint、Schema 对象精确集合 | `require_current_runtime_state`、`validate_encrypted_values`、`database_schema.rs` | 保障 | 高 | 局部损坏、错误 key、密文跨行替换、裸 SHA fingerprint、非规范 Host 或额外对象会潜伏到远端操作阶段 | integrity/FK、错 key/空 AAD/跨记录密文、裸 SHA/错 HMAC、Host 漂移、operation/action mismatch、缺失/额外/修改对象 |
| SUN-072 | 只在数据库文件不存在时一次性创建当前 DDL；现有 main/WAL/journal 先复制为稳定私有代际并在副本预检，源 SHM 只查文件身份 | `open_or_initialize`、`snapshot_generation`、`initialize_empty` | 核心 | 高 | 若对任意存量库补表/改列，就重新引入产品内迁移；若直接以 SQLite 打开待拒绝源库，会改写 SHM 锁字节 | 缺失路径创建、WAL-only current/非当前、空文件/漂移零字节变化、代际竞态、sidecar link、失败清理 |
| SUN-073 | SQLite PRAGMA、WAL/checkpoint 与连接行为统一来自 Foundation | `sarmg-sqlite` | 保障 | 中 | 各产品 busy/foreign-key/durability 行为再次分叉 | reopen、busy、FK、checkpoint |
| SUN-074 | 运行锁与 maintenance 排他锁保护单实例/离线维护 | `runtime_lock`、database sibling lock | 保障 | 高 | 两个进程可重复 claim、写库或在检查时看到变化状态 | 双 serve、doctor/维护冲突、异常退出释放 |
| SUN-075 | bootstrap 只在零账户时创建当前管理员，之后仍验证配置 username 与全部存量管理员合同 | `ensure_admin_user`、`admin-create` | 保障 | 高 | 启动可能暗中忽略非法配置、覆盖账户或接受非当前 username/hash；管理命令可能谎报重复创建成功 | 首次创建返回 true、重启返回 false、非法配置即使已有账户也拒绝、`admin-create` 已有账户拒绝、错误存量零写入 |
| SUN-076 | static root 必须绝对路径且顶层恰好 `index.html`/`assets` | `validate_static_dir` | 保障 | 中 | 启动可能提供错误构建、源码或残留文件 | 相对路径、缺项、额外顶层项 |
| SUN-077 | static tree 深度≤32、条目≤10000、只含真实目录/普通文件 | `validate_static_tree` | 保障 | 中 | symlink/hardlink/巨树可越界读取或制造 TOCTOU/DoS | link、设备文件、深度/数量、nlink |
| SUN-078 | 正式 static 资产非服务账户所有且不可 group/world 写 | Unix metadata validation | 保障 | 中 | 被攻陷服务进程可篡改下次响应的管理 UI | uid、022 mode、开发模式差异 |
| SUN-079 | 联调期 Rust 使用 Foundation 0.4.0 path；正式发行必须将 Rust/Web 同时锁定 Foundation 0.4.0 完整 Git revision 和 GitHub Release tarball | `Cargo.toml`、`clients/web/package.json`、lockfiles、`.node-version`、toolchain check | 开发运维 | 中 | 未发布期间伪造 revision/URL，或在正式发行中留下 path/file/多版本会破坏可复现供应链 | 联调测试；发布后 clean Cargo locked resolve、`npm ci`、URL/version/integrity 与无 path/file 扫描 |
| SUN-080 | 认证状态机统一使用 Foundation admin-web | `@sarmg/admin-web`、`App.tsx` | 保障 | 高 | 登录/恢复/401/退出竞态会在各产品重复实现并分叉 | stale response、重登、卸载、no-store |
| SUN-081 | Web 请求固定 same-origin、严格响应 guard 和超时/大小边界 | Foundation http-client/contracts | 保障 | 中 | 可被配置成跨 origin 发送 Cookie，或把畸形响应当合法数据 | absolute URL 拒绝、错 shape/MIME/超时 |
| SUN-082 | 当前 Web 在认证丢失后清空 Host 列表和错误状态，并取消慢 Host 响应回填 | `clients/web/src/App.tsx` effect cleanup | 保障 | 中 | 下一位使用同一浏览器的人可能看到上一管理员的 Host 投影；当前 Web 没有 operation 状态 | 慢请求、restore error、logout/login overlap |
| SUN-083 | 外部封面 URL 仅允许配置的精确 HTTPS hostname | `CoverUrlPolicy`、allowlist | 保障 | 高 | 可利用服务端访问任意站点；删除功能时应关闭入口而非允许全部 | scheme、userinfo、port、hostname、空 allowlist |
| SUN-084 | 每次解析要求全部地址均为公网且拒绝混合集合 | cover policy/resolver | 保障 | 高 | 攻击域名可解析到 loopback、RFC1918、link-local 或 metadata | IPv4/IPv6 私网、混合、公网 |
| SUN-085 | 下载执行前重新解析并固定目标，禁止 DNS rebinding | cover downloader/pinned resolution | 保障 | 高 | allowlist 检查后域名可切换到内网 | 两次 DNS 不同、连接目标断言 |
| SUN-086 | 封面下载 no-redirect、有界 timeout/8 MiB、图片 MIME | cover client | 保障 | 中 | 可通过跳转绕过 allowlist或用慢/大正文耗尽资源 | 30x、错 MIME、慢流、分块超限 |
| SUN-087 | 内部回取 token 30 秒一次性并绑定 Host/operation/peer | `CoverProxy::publish/take`、internal route | 保障 | 高 | token 可跨 Host 重放、长期读取或被非目标 Sunshine 使用 | 二次使用、过期、错四元组 |
| SUN-088 | 内存封面最多 16 项、Host 解析地址最多 16 个 | `MAX_ENTRIES`、`MAX_HOST_ADDRESSES` | 保障 | 中 | 大量上传或 DNS 放大可耗尽内存/连接尝试 | 第 17 项/地址、消费后释放 |
| SUN-089 | `/healthz` 与 `/readyz` 独立于管理员 API；ready 只重验当前 Schema | `live`、`ready`、`db::ready` | 开发运维 | 低 | 编排器无法区分进程存活和 Schema readiness；它不代表 worker、Host 或全部密文实时健康 | 匿名访问、Schema drift 503、Host/worker 故障仍说明边界 |
| SUN-090 | `doctor` 验证配置/static、数据库 identity/integrity/FK、回滚写探针和全部密文 | `ServeConfig::from_runtime`、`db::doctor` | 开发运维 | 中 | 部署验收与事故定位只能靠启动或手工 SQL；命令会执行回滚事务但不留业务探针，也不验证整个 release tree | 健康/错 key/Schema/FK/不可写/static 错误、探针行不留存 |
| SUN-091 | 源配置固定 `config/`，部署资产固定 `deploy/`，正式 env 固定 `/etc/isarmg/sunshine-manager.env` | repository/deploy convention | 开发运维 | 低 | 多项目安装和审查需要记忆不同目录，自动化容易漏项 | 文档/脚本/service 路径扫描 |
| SUN-092 | systemd 使用专用账户、受保护环境、固定 release 路径和 hardening | `deploy/sunshine-manager.service` | 保障 | 中 | 服务获得不必要主机权限，或读取可变工作树 | `systemd-analyze verify`、权限、负面写入 |
| SUN-093 | Server binary 与随附 Web 正式发行只允许 `x86_64-unknown-linux-gnu` | `sarmg-server-target`、`build.rs`、release/CI | 核心 | 中 | 重新增加 ARM/其他 Server target 会扩大未验证的系统与发行矩阵；受管 Sunshine/Moonlight 外部端平台和协议不变 | AMD64 正例、其他 Server target 负例、manifest、外部端边界文档 |
| SUN-094 | binary/release 绑定源码 revision、target、API 与 Schema | `release_contract`、binary self-report | 保障 | 高 | 不同提交或合同的二进制、Web 和 service 可被拼装 | 40-hex revision、字段逐项篡改 |
| SUN-095 | release tree 固定完整文件集合、mode、size、SHA-256 | `package-release.py`、manifest verifier | 开发运维 | 高 | 缺失、额外或篡改文件可能进入生产 | missing/extra/tamper/mode/hardlink/symlink |
| SUN-096 | 版本目录不可变、拒绝同版本覆盖和 mutable alias | 打包/安装合同、运维流程 | 保障 | 中 | 运行内容无法由版本名唯一定位，回滚与取证失真 | 同版本二次安装、`current` 链接、重定位 |
| SUN-097 | CI 固定 Rust 1.98、Node 26.7.0、锁文件和 action full SHA | workflow、toolchain files、supply-chain check | 开发运维 | 中 | clean checkout 结果不可复现，依赖漂移可绕过评审 | locked install、action/ref/工具链扫描 |
| SUN-098 | 中文学习指南、流程树、完整清单、README、运维文档同代码维护 | `docs/`、README | 开发运维 | 中 | 关键状态语义和删除闭包依赖口头知识，开发者会误判边界 | 链接、命令、代码符号与版本抽查 |
| SUN-099 | 产品只接受唯一当前配置/API/Schema/key/release 身份 | 各入口 fail-closed 校验 | 核心 | 高 | 加入 fallback 会把每次发布变成长期多版本产品并扩大安全矩阵 | 非当前状态逐类拒绝、拒绝时零写入 |
| SUN-100 | 明确不提供 Moonlight 媒体、Host OS 任意命令、SSO、多活、产品内迁移/备份/恢复 | route/依赖/Schema 中不存在这些能力 | 核心 | 高 | 任一新增都会改变威胁模型、带宽、身份或一致性架构 | 单独设计评审、数据/故障/安全全链路测试 |

## 1. 功能清单

| 领域 | 当前能力 | 取舍/限制 |
|---|---|---|
| 身份 | 唯一本地管理员角色 `admin`、Argon2、Session、CSRF、登录限流 | 不依赖共享 SSO；没有 viewer/operator 或权限矩阵 |
| Host | 新增、修改、删除 Sunshine 连接与凭据 | 远端是非事务性不可信 peer |
| 客户端 | 查看和管理 Sunshine 客户端 | 能力受 Sunshine API 支持范围约束 |
| 应用 | 查询、创建、修改、删除应用配置 | mutation 全部异步，不同步伪装成功 |
| Operation | durable、幂等、per-Host 串行、恢复、状态查询 | `unknown` 需要人工核对 |
| 审计 | requested/completion 与 durable outbox | 不记录 Secret 或远端正文 |
| 封面 | HTTPS allowlist、DNS 公网校验、pin、一次性代理 | 需要 Sunshine 主机可达内部 HTTPS origin |
| 数据 | 当前 SQLite、字段密文、运行锁、doctor | 单数据库单活，不是集群数据库 |
| Web | Foundation React/Vite 管理员登录、Session 和 Host 只读概览 | 当前不提供 Host CRUD、客户端、应用、operation 管理 UI，也不提供插件系统 |
| 发布 | 固定目录、source-bound、全树 manifest | 同版本不覆盖，无 mutable alias |

## 2. 架构取舍

- 使用 durable operation 而不是同步远端写入，准确表达网络不确定性；UI 和调用方必须处理异步状态。
- 每 Host 串行避免同一 Sunshine 的写入乱序，不同 Host 并发维持吞吐；单 Host 慢操作会形成局部队列。
- SQLite 降低独立部署成本，依靠单实例锁保证执行互斥；不支持多个进程共享同一库做 active-active。
- 精确 AES-256-GCM 受认证密文保护数据库泄漏场景；确定性、长度分帧的 AAD 同时把 Host credential
  绑定到 Host ID/字段域，把 operation request 绑定到 operation ID/action/字段域，阻止合法密文跨记录
  替换。external key 丢失无法恢复，因此密钥独立保管是业务连续性的硬要求。
- Request fingerprint 与 Idempotency-Key lookup 不保存裸 SHA-256；master key 经 HKDF-SHA-256 的两个
  固定独立 info 派生 HMAC key，既维持稳定 32-byte 比较/索引，又避免数据库副本对低熵 PIN/键做字典验证。
- 封面由一次性反向代理隔离原始 URL，降低 SSRF；需要额外 DNS/egress 和代理拓扑配置。

## 3. 安全边界

默认 loopback listener，生产由可信 HTTPS proxy 暴露并使用 Secure Cookie。所有 Sunshine 主机连接
固定使用 HTTPS，并由平台信任库强制验证证书链、有效期与主机名；API、数据库和 Web 没有关闭校验的
字段。外部 URL allowlist 不是 Sunshine egress firewall 的替代。
数据库、credential key、环境文件和 release 分别以最小权限保护。

Server 不终止浏览器侧 TLS，也不自行注入 HSTS/CSP；这些响应策略属于可信 HTTPS proxy 的当前责任。
应用层 `ErrorEnvelope` 允许可选 request ID，但本版本没有生成器，正常响应中该字段缺省；不要在调用方
假设一定存在。

## 4. 当前版本边界

- 只注册 `/api/v2/auth/*` 与 `/api/v2/sunshine/*`。
- 只接受 `sunshine-manager 0.8.0`、Schema revision 2、SHA-256
  `c9dedb33dd7a5ad613e762eb135a7aa5184ce1df52166459bee7b3485b4b3be3`。
- 只接受配置的当前 credential key ID/key；没有 previous-key keyring。
- 只接受当前带记录身份 AAD 的 `sunshine:sgev1:` envelope；相同文本前缀的空 AAD/非当前密文仍会认证失败，
  不通过降级解密、试错 AAD 或跨行重加密兼容。
- 只接受当前 HKDF 分域 HMAC-SHA-256 request fingerprint/Idempotency-Key hash；没有裸 SHA-256、共用
  HMAC key、previous master key 或失败后试算其他算法的分支。
- 不读取 migration ledger、非当前数据库或未知发行 manifest。
- 产品二进制不包含 backup、restore、migration、key rotation 或 re-encryption；精确当前状态备份/恢复由 `sarmg-upgrade` 实现。

Foundation 只提供当前版本的共享原语，不是在线服务或产品框架。应用层 `AppError` 固定输出
`{code,message,retryable,request_id?,details?}`，未知字段不属于当前合同；Axum extractor/layer rejection
当前仍是框架响应并会被 Foundation Web client fail-closed 拒绝。Web 使用同源、有界 HTTP client，但
Session 与 CSRF Token 生命周期由 Foundation Admin Core 持有。Foundation 负责 SQLite 连接 PRAGMA、
integrity/FK/checkpoint 和 Schema identity 算法；本项目继续负责数据库路径/权限、私有代际快照预检、DDL、初始化、
失败清理、运行锁和外部升级仓边界。

Foundation 校验后的 current Schema identity 会进入 Sunshine 的 binary release identity。通用 release-tree
工具不能无损表达本项目的必需目录、binary self-binding、0555/0444、总字节与重定位规则，因此没有直接
替换 Sunshine 的 source-bound verifier；后者继续负责文件 mode/size/SHA-256、symlink/hardlink 拒绝和
篡改验证。没有引入 `upgrade_tool` 或运行时 Foundation 进程。

## 5. 明确不提供

不传输 Moonlight 媒体、不管理 Host OS、不远程执行任意命令、不自动重试 unknown、不允许任意封面 URL、
不提供容器/共享中央运行时，也不为非当前 API、Schema 或 Secret 添加读取分支。

## 6. Host 连接能力详解

| 项目 | 当前行为 | 安全/可靠性理由 | 操作者责任 |
|---|---|---|---|
| Endpoint | `https://<host>:<web_port>` | 不支持明文 Sunshine API | 配置可解析 DNS/IP 与端口 |
| TLS | 总是验证链、有效期和主机名 | 不允许中间人窃取 credential/修改操作 | 私有 CA 先纳入平台信任库 |
| Credential | 当前 external key 认证加密 | 数据库泄露不直接暴露密码 | 独立备份 key，限制环境权限 |
| Timeout | 有界连接/总请求 | 慢 Host 不无限占 worker | 监控网络和 Host 延迟 |
| Redirect | 禁止 | 防 credential/请求被转发到未知 origin | 配置最终规范地址 |
| Response | 状态、Content-Type、大小、JSON 形状验证 | Sunshine 是不可信网络 peer | 版本/响应异常先隔离 Host |

创建/修改 Host 的 API 与数据库没有 TLS 校验开关，未知 API 字段会被严格拒绝；当前 Web 根本没有 Host
编辑表单，因此也不存在 UI 开关。即使在开发模式，Sunshine 上游 TLS 验证也不会放宽；开发模式只影响
本地浏览器 Cookie/监听边界。

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
| dead_letter/resolved | 重试预算耗尽、封存或经人工确认 | 保留审计与确认者，不伪造原结果，不重放 |

相同 actor/Host/action/Idempotency-Key 与相同请求返回原 operation；不同请求复用键返回冲突。当前 Host
CRUD 没有 revision 或 `If-Match`，同一进程内虽按 Host 加锁，两个已登录页面仍是后写覆盖先写。若未来要
防止过期页面覆盖，必须新增明确的当前并发合同、数据库约束、409 语义与 Web 测试，不能在文档中假定存在。

## 9. 认证和审计功能

| 能力 | 当前保证 | 限制 |
|---|---|---|
| 登录 | canonical username、Argon2、未知账户等成本、来源/账户预算、进程级 Argon2 并发 | 本地仅 `admin` 角色，不共享 SSO，不接受邮箱身份 |
| Session | 随机 Token 摘要、idle/absolute TTL、撤销 | 不跨项目/跨部署共享 |
| CSRF | unsafe method 需绑定 Token + Origin/Host | 可信代理必须保持正确 Host |
| 审计 | requested/completion/resolved durable outbox 幂等物化到本地 `audit_logs` | 不记录 Secret、encrypted request 或上游正文；没有外部 sink |
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

| 数据 | 存储 | 当前验证/连续性责任 |
|---|---|---|
| Host metadata | SQLite `hosts` | 当前 Schema、普通字段边界 |
| Host credential | 当前 envelope ciphertext | external key、Host ID 与 `secret` 字段域共同认证全部记录 |
| Operation request | 当前 envelope ciphertext | external key、operation ID、action、字段域共同认证；覆盖所有状态 |
| Request fingerprint | 32-byte HMAC-SHA-256 BLOB | 独立 HKDF info 派生 key；启动重算、constant-time 比较，不保存裸 SHA |
| Idempotency-Key lookup | 32-byte HMAC-SHA-256 BLOB | 另一 HKDF info 派生 key；SQLite 在 actor/Host/action 范围内精确匹配 |
| Session | Token/CSRF 摘要 | TTL、版本、撤销 |
| Audit/outbox | SQLite | 与业务事务一致、可幂等物化到本地 `audit_logs`；未实现外部导出 |
| External key | 数据库外受保护配置 | raw bytes 不进数据库、release、日志或 support bundle |

当前数据库 SHA 为上文列出的 code-owned 值。数据库副本只有与正确 external key 配对才有技术意义；但
当前 `sarmg-upgrade` 提供 Sunshine 0.8.0 keyed backup/verify/restore，但没有已登记的历史 upgrade edge；
绕过该工具复制文件不能被描述成可恢复承诺。
只改 metadata 或 key ID 不能让错误状态变合法。

`sunshine:sgev1:<key-id>:<base64(SGEV envelope)>` 是存储 envelope，不是完整认证合同。当前实现为每个
密文确定性构造 AAD：先加入固定格式域 `sunshine-manager:aes-256-gcm:aad:v1`，再加入用途域和记录组件；
每个组件都以前置 64-bit big-endian 长度编码，避免分隔符歧义。Host 组件是 `host-credential`、Host ID、
`secret`；operation 组件是 `operation-request`、operation ID、action、`request_ciphertext`。AAD 不写入
SQLite，因此数据库 DDL 和 Schema SHA 不因这次合同收紧而改变；行身份/action 变更后，原 ciphertext
必然无法认证。空 AAD reader、旧密文 reader 或“失败后尝试其他 context”均不属于本产品。

HKDF 的固定 salt 为 `sunshine-manager:credential-master-key:hkdf-sha256:v1`；request fingerprint 与
Idempotency-Key hash 分别使用
`sunshine-manager:operation-request-fingerprint:hmac-sha256:v1` 和
`sunshine-manager:operation-idempotency-key-hash:hmac-sha256:v1` 作为 info。两把派生 key 不持久化、不
从通用接口暴露。相同输入在各自域内稳定、跨域不同，换 master key 后全部改变。Request fingerprint
比较使用 constant-time；Idempotency-Key HMAC 由 SQLite 作为完整 32-byte BLOB 查找。两列长度未变，
因此 DDL 和 Schema SHA 仍保持当前值。

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
| external key、记录 AAD 或密文错误 | 启动/doctor 失败；worker 不会发出无法认证的请求 | 尝试其他 key、空 AAD 或其他记录 context 的 fallback |
| request fingerprint 为裸 SHA/错 HMAC | 启动/doctor 失败；同键比较只认当前 HMAC | 失败后试算裸 SHA、另一 HKDF 域或旧 master key |
| Schema/manifest drift | 启动前拒绝 | 手改 SHA/忽略额外文件 |
| 封面 DNS 变化 | 下载拒绝 | 只信首次解析 |
| Outbox 本地物化失败 | 业务状态保留，后台循环按稳定 ID 重试 | 丢弃 outbox 或假定已有外部 sink |

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

新能力必须具备严格 API、单管理员授权、Session/CSRF、durable operation 与幂等、Sunshine 响应边界、
Secret 加密、容量/超时、审计 outbox、重启/unknown、当前 Schema 与外部升级边界、Web 行为、release
篡改测试和中文运维。直接在
handler 中调用一次 Sunshine 并返回 200 不构成完整功能。

## 16. 当前 HTTP 能力台账

下表是 route 审查入口，不替代 Rust DTO。除健康检查、登录和一次性封面交付外，所有 route 都经过同一
管理员 Session；所有 unsafe route 还经过 CSRF 与 same-origin 校验。

| Route 组 | Method/路径 | 身份与请求边界 | 权威状态/结果 |
|---|---|---|---|
| 健康 | `GET /healthz`、`GET /readyz` | 匿名、无业务数据 | 进程与 readiness，不泄露 Host |
| 认证 | `POST /api/v2/auth/login` | 匿名但需同源、16 KiB、登录预算 | `AdministratorSession(role=admin)` + HttpOnly Cookie |
| 认证 | `GET /api/v2/auth/session` | Session | 轮换 CSRF 摘要并返回新 token |
| 认证 | `POST /api/v2/auth/logout` | Session + CSRF + 同源 | 撤销 DB Session，204，过期 Cookie |
| Host | `GET/POST /api/v2/sunshine/hosts` | admin；POST strict DTO | 列表投影或本地事务创建 + audit |
| Host | `PATCH/DELETE /api/v2/sunshine/hosts/{id}` | admin + CSRF；无 revision | 本地事务后写覆盖/删除 + audit |
| 探测 | `GET .../hosts/{id}/status` | admin | 内存健康快照，不发起同步写 |
| Applications | `GET/POST .../apps`、`POST .../apps/close`、`DELETE .../apps/{index}` | GET 同步读取；mutation 需唯一幂等键 | mutation 返回 202 Operation |
| Clients | `GET .../clients`、`POST .../clients/unpair[-all]`、`POST .../clients/update` | UUID≤128；mutation 需唯一幂等键 | 读取严格投影或 202 Operation |
| Config/logs | `GET/POST .../config`、`GET .../config/locale`、`GET .../api-logs` | config object≤1 MiB；上游正文≤4 MiB | 读取同步；保存为 Operation |
| Pair/system | `POST .../pin`、`.../restart`、`.../reset-display` | PIN 4–8 digits、name≤80、幂等键 | 202 Operation，可能 unknown |
| Cover | `GET .../covers/{index}`、`POST .../covers/upload` | index≤10000；上传 strict DTO + 幂等键 | 读取≤8 MiB；上传为 Operation |
| Cover internal | `GET .../internal/hosts/{host}/operations/{op}/covers/{token}` | transport peer + 30s token 四元绑定 | 一次性图片，永不暴露 external URL |
| Operation | `GET .../operations/{id}` | actor-bound admin | 脱敏 `OperationView` |
| Operation | `POST .../operations/{id}/resolve` | admin + CSRF；unknown/dead-letter | resolved + evidence outbox，不重放 |

删除某个 route 时，必须同步删除 Web 调用、DTO、operation enum/action、数据库约束或索引、上游 client、
审计事件、测试和文档。只从 router 隐藏 route 会留下不可达持久状态和维护负担。

## 17. Mutation 风险与重试矩阵

| Action | 可能副作用 | 自动重试 | 人工 retry API | 断线默认 | 删除闭包重点 |
|---|---|---|---|---|---|
| app save | 新增/覆盖应用 | 无 | 无 | unknown | 删除 AppsSave enum、route、Web、测试 |
| app close | 结束当前会话 | 无 | 无 | unknown | UI 仍需刷新 actual state |
| app delete | 删除远端应用 | 无 | 无 | unknown | 清除 index 校验和 delete route |
| client unpair | 撤销一个客户端 | 无 | 无 | unknown | 保留客户端列表读取或同时删域 |
| client unpair all | 撤销全部客户端 | 无 | 无 | unknown | 风险高，删除需同步 Web 确认 |
| client update | 启停客户端 | 无 | 无 | unknown | enabled DTO 与上游 method 同删 |
| config save | 覆盖远端配置对象 | 无 | 无 | unknown | 删除 1 MiB object 校验与 action |
| PIN pairing | 新建配对 | 无 | 无 | unknown | PIN/name 校验与审计同删 |
| restart | 重启 Sunshine | 无 | 无 | unknown | 删除后不影响静态管理，但失去运行控制 |
| reset display | 清除显示持久状态 | 无 | 无 | unknown | 删除专用上游 path 与按钮 |
| cover upload | Sunshine 主动回取一次性 URL | 无 | 无 | unknown | policy/proxy/token/egress 是一个整体 |

不提供人工 retry API 或兼容入口。Unknown 必须先根据远端事实人工 resolve；终态不会重新变成 pending。
新意图必须由管理员明确发起，不能用新幂等键自动重放结果不确定的旧意图。

## 18. 删除功能时的依赖闭包

| 拟删除范围 | 必须同时检查 | 不能留下的半实现 |
|---|---|---|
| 全部远端 mutation | operation/outbox worker、Idempotency-Key、Web polling、operations DDL | 只有表和后台线程但无入口 |
| 某个 action | enum、action string、executor、safe-retry allowlist、route、Web、测试 | 数据库能保存但 worker 不能执行的请求 |
| 外部封面上传 | allowlist config、DNS policy、download client、proxy origin/token、internal route | URL 校验存在但无消费者，或反向情况 |
| Host credential | Host CRUD、AES-GCM、external key config、doctor、release/运维 | 明文 password 或无法认证的密文 |
| Web 控制台 | static_dir/startup verifier、React 依赖、release web tree、CSP/proxy 文档 | Server 仍要求不存在的 `web/` |
| 管理员认证 | users/sessions、登录预算、CSRF/origin middleware、admin-web、Cookie proxy | 部分 route 匿名、部分仍期待 identity |
| 本地审计 | `audit_logs` 与 operation `audit_outbox`、doctor/Schema | 删除一张表却保留 trigger/index/写入语句 |
| SQLite | 全部事务、锁、operation 恢复、Schema identity、doctor | 不能用另一数据库“适配层”假装等价 |
| source-bound release | build.rs、identity、packer/verifier、systemd 路径、CI | 开发 binary 被误当正式 binary |
| current-only 约束 | API/Schema/key/release/config 所有入口 | 任一 fallback 会重新扩大完整测试矩阵 |

## 19. 分类和复杂度解释

- **核心**：删除后产品定义或主要数据流不成立；不是“代码重要程度”。
- **保障**：删除后主功能可能仍能演示，但安全、确定性、容量或取证边界失效。
- **可选**：可在不破坏主闭包的前提下整体删除；仍必须清理完整依赖闭包。
- **建议保留**：并非最小控制面所必需，但删除会显著增加人工操作、事故时间或开发分叉。
- **开发运维**：构建、发行、验收、文档与故障定位能力；不直接产生业务数据但决定可交付性。

复杂度“低/中/高”按跨层联动评估：单一纯函数通常为低；涉及 route 与一侧状态为中；跨浏览器、数据库、
异步 worker、远端副作用、加密或发行身份任意三个层面即为高。它不是工时承诺，删除高复杂度功能通常也
需要高复杂度验证。
