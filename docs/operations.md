# Sunshine Manager 运维文档

## 1. 生产布局

```text
/opt/isarmg/sunshine-manager/releases/0.8.0/  root-owned read-only release
/etc/isarmg/sunshine-manager.env             0600 environment
/var/lib/isarmg/sunshine-manager/db/sunshine-manager.sqlite3  SQLite
/run/isarmg/sunshine-manager/                locks/runtime
```

systemd 使用 `isarmg-sunshine`，直接执行：

```text
/opt/isarmg/sunshine-manager/releases/0.8.0/bin/sunshine-manager \
  serve-release --root /opt/isarmg/sunshine-manager/releases/0.8.0
```

发行树无 `current` 链接，不依赖工作目录，必须拒绝 symlink、特殊文件、hardlink asset、服务账户拥有的
asset 和 group/world writable 内容。

## 2. 构建与安装发行物

从干净且 annotated `v0.8.0` 精确指向 HEAD 的 checkout：

```bash
python3 scripts/package-release.py /absolute/release-output
```

本迁移工作树联调 Foundation 0.4.0 Rust crate 和本地生成的四个 0.4.0 npm tarball；Web manifest
与 lockfile 已预先绑定将要发布的 `v0.4.0` URL 和这些 tarball 的 SHA-512 integrity。正式发行前必须
等 Foundation `v0.4.0` 指向不可变完整 revision，将 Cargo 的联调 `path` 统一替换为该 revision，
并在无 sibling Foundation 的干净环境执行 `npm ci`。在这些条件满足前，不得将 Sunshine 0.8.0
工作树标记为正式发行。
Foundation 不是生产运行服务，发行树中不增加其 daemon、配置或 socket。

输出已存在时拒绝覆盖。安装：

```bash
tar -xzf sunshine-manager-0.8.0-x86_64-unknown-linux-gnu.tar.gz \
  -C /opt/isarmg/sunshine-manager/releases
/opt/isarmg/sunshine-manager/releases/0.8.0/bin/sunshine-manager \
  verify-release --root /opt/isarmg/sunshine-manager/releases/0.8.0
```

安装仓库内 systemd unit，创建专用账户、状态目录和 `/etc/isarmg/sunshine-manager.env`，再 enable/start。
同版本不得合并或覆盖。

正式归档只生成 `x86_64-unknown-linux-gnu` Server 与随附 Web。该限制不要求受管 Sunshine Host 或
Moonlight 客户端使用 AMD64，也不改变 Sunshine 上游 API；它们仍是本控制面的外部端。

## 3. 环境配置

| 变量 | 默认/要求 | 说明 |
|---|---|---|
| `SUNSHINE_MANAGER_DATABASE_URL` | 必填 | 当前 SQLite URL |
| `..._BIND` | `127.0.0.1:18104` | 生产回环监听 |
| `..._STATIC_DIR` | 必填固定 release Web | 不能是 symlink |
| `..._CREDENTIAL_KEY_ID` | 当前 key ID | 必须与当前库中全部 envelope 一致 |
| `..._CREDENTIAL_KEY` | Base64 32 字节 | 独立秘密管理，禁止日志/仓库 |
| `..._BOOTSTRAP_ADMIN_USERNAME` | `admin` | 1–64 字节 printable ASCII candidate；解析后必须得到 3–64 字节 canonical username |
| `..._BOOTSTRAP_ADMIN_PASSWORD` | `_sarmg_administrators` 为空时必填 | 12–1024 字节且无 ASCII control；使用后立即轮换初始密码 |

Session、Cookie、CSRF 和登录限流使用 Foundation `AdministratorPolicyV1` 固定值，产品环境变量不能覆盖。
| `..._COVER_URL_ALLOWLIST` | 默认空 | 逗号分隔精确 DNS host |
| `..._COVER_PROXY_ORIGIN` | allowlist 非空时必填 | Sunshine 主机直达 HTTPS origin |

## 4. 管理命令

```bash
sunshine-manager identity
sunshine-manager verify-release --root /opt/isarmg/sunshine-manager/releases/0.8.0
sunshine-manager doctor
sunshine-manager admin-create --database-url sqlite:///path/app.db
sunshine-manager admin-reset-password --database-url sqlite:///path/app.db \
  --username admin --password '<new-secret>'
```

管理员写命令需要 maintenance exclusive；先停服务。避免把真实密码留在 Shell history，使用受控 Secret
注入或临时受保护终端。`admin-create` 只允许数据库中尚无管理员时创建首个账户；已有管理员时它会拒绝，
不会把“校验现有账户”伪装成新建成功。`admin-reset-password` 只接受 canonical 化后精确匹配的当前 username。

管理员身份不是邮箱。登录候选必须为 1–64 个可打印 ASCII 字节；Server 只执行 `trim_ascii()` 和
ASCII 小写化，然后要求持久值为 3–64 字节、首尾是字母/数字且字符只来自 `[a-z0-9._-]`。`@`、Unicode、
控制字符和其他符号均拒绝；数据库 CHECK、启动存量检查、Session DTO、账户限流键和 Web 表单使用同一
username。不存在 `EMAIL` 环境变量、`--email` 参数、JSON `email` 字段或兼容别名。

## 5. Doctor

`doctor` 验证 product metadata、现场 Schema fingerprint、SQLite integrity/foreign keys、可回滚写事务，
并复验 Host 规范字段、用当前 key 认证全部 Host credential 和全部持久 operation request；operation
plaintext 还必须能解析为当前严格 enum 且 action 与行一致。认证上下文必须精确匹配：Host 使用 Host ID
和 `secret` 字段域，operation 使用 operation ID、action 和 `request_ciphertext` 字段域。密文被复制到另一
记录、action 被修改、用途错误或使用空 AAD 时都失败；不会尝试旧格式或不带 AAD 的 fallback。它不连接
Sunshine。扫描还会从已解密的每条 operation request 重算 HKDF 分域 HMAC-SHA-256 fingerprint 并用
constant-time 比较；裸 SHA-256 或另一 master key 生成的 fingerprint 会使 doctor/启动失败。它不保留
业务探针行；它不是纯只读命令，因为会执行随后回滚的写事务。

其中 WAL/FULL synchronous/foreign-key/busy-timeout 连接基线、checkpoint、integrity/FK 和 Schema 指纹算法来自
Foundation 0.4；数据库文件权限、main/WAL/journal 私有代际快照预检、DDL/init、失败清理、运行/maintenance
锁仍由 Sunshine Manager 负责。预检只读取源 `-shm` 的类型与身份，不在源库上建立 SQLite 连接；因此非当前
库拒绝不会改写 SHM 锁字节。故障定位时不要绕过任一层，也不要用 Foundation API 现场创建或转换非当前库。

若 Schema 不符，不得手改 metadata；若解密失败，先确认 key ID、key 文件来源和权限，不得添加“尝试
其他 key”、空 AAD 或忽略记录身份的 fallback。即使 envelope 仍以 `sunshine:v1:` 开头，也不能据此前缀
判定它属于当前合同；AES-GCM tag 必须在当前确定性 AAD 下验证通过。
同一 master key 除直接供 AES 使用外，还派生两把 HMAC key，但不会暴露通用 HMAC key API：request fingerprint
和 Idempotency-Key hash 各有固定且不同的 HKDF info。换 master key 会同时改变密文可用性和两个 HMAC 域，
不得手改 BLOB、回退裸 SHA-256 或尝试另一域的 key。

## 6. 反向代理和网络

公网 TLS proxy 保留原 Host，并确保 Session Cookie Secure；同时由 proxy 设置经验证的 HSTS/CSP 等浏览器
响应策略，Server 当前不终止浏览器 TLS 或注入这些 header。应用端口只对 proxy 回环开放。Sunshine
Host 地址由防火墙限制。Manager 对每次 Sunshine 连接都强制使用 HTTPS，并验证证书链、有效期和主机名；
这不是“优先项”，也没有开发或单 Host 绕过开关。私有 CA 必须先安全安装到服务进程实际使用的系统信任库。

封面代理 origin 必须由 Sunshine Host 直接访问，不能把一次性路径经过公共 proxy；两端网络都要拒绝
private/link-local/loopback/metadata egress 和危险 redirect。

## 7. 当前连续性限制与外部升级边界

Sunshine Manager 产品仓没有 backup、restore、Schema conversion、key rotation 或 re-encryption 命令。
`sarmg-upgrade` 是这些能力的唯一所有者，已实现 Sunshine 0.8.0 精确当前 SQLite 状态的 keyed
backup/verify/restore。数据库与 32-byte external key 必须作为一个安全单元保全，并且只能使用
该工具的显式命令；文件复制、SQLite `.backup` 或手工换 key 不属于受支持流程。

需要新环境时，可以恢复经 `sarmg-upgrade` 严格验证的 0.8.0 当前备份，或创建全新当前数据库并重新登记
Host；不要把非当前库交给 Server，也不要逐表复制。本版本不包含任何历史 edge、key rotation 或
re-encryption，产品仓也不增加兼容分支。

## 8. 监控与故障定位

1. 检查 systemd、release verify 和 Web asset 是否通过。
2. 检查反向代理 TLS、Cookie、Origin/Host 与系统时钟。
3. 运行 doctor，确认 SQLite、写能力和全部密文。
4. 对 pending 查看 worker/Host 网络；对 unknown 先查 Sunshine 实际状态，禁止盲重试。
5. 对封面错误检查 allowlist、DNS 的全部地址、MIME/大小、proxy origin 和 Host egress。
6. 监控 SQLite/WAL、operation backlog、unknown 数量、磁盘和 inode。

API 错误必须同时检查 HTTP status 与稳定 `code`；`message` 只用于展示。若 Web 报
`invalid_error_response`，先检查代理是否改写 JSON/content-type 或服务端是否返回非当前错误形状；若为
`invalid_response_shape`，说明 2xx 正文已偏离当前端点合同，不应在浏览器添加宽松分支。
`request_id` 是 Foundation envelope 的可选字段，本版本 Server 不生成；需要链路关联时应由可信 proxy/
日志平台生成并在其自身受保护日志中维护，不能伪造为产品当前 API 的必填字段。

## 9. 安全事件

隔离公网和受影响 Sunshine Host，保全 release SHA、数据库 generation、审计和日志，撤销管理员 Session，
并轮换管理员密码、Sunshine 凭据与 TLS key。credential key 一旦确认泄露，而升级仓又没有已审计的 re-encryption edge，
应停用该数据库，建立全新当前数据库/key 并重新登记 Host，不能继续运行或自行批量改密文。使用 GitHub Private
Vulnerability Reporting；公开 issue 不得包含生产 Host、数据库、密文、key、URL 或请求正文。只支持
当前发布版本与当前 `main`。
