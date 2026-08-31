# Sunshine Manager 工作流程与流程树

## 1. 总流程树

```text
Sunshine Manager
├─ 启动
│  ├─ verify immutable release/Web
│  ├─ parse SUNSHINE_MANAGER_* config
│  ├─ instance lock + maintenance shared lock
│  ├─ current SQLite/schema/credentials check
│  └─ HTTP + operation workers + audit outbox
├─ 内置 Web
│  ├─ login/restore/logout -> Session + CSRF
│  └─ read-only Host overview
├─ 管理 API 调用方
│  ├─ read Hosts/clients/apps/config/logs/covers
│  └─ mutation + Idempotency-Key -> operation
├─ Worker
│  ├─ per-Host serialization
│  ├─ decrypt request/credential
│  ├─ call Sunshine
│  └─ succeeded/failed/unknown + audit
└─ 运维
   ├─ identity/verify-release/doctor
   └─ 当前部署/doctor；未来转换仅由 sarmg-upgrade 的显式 edge 提供
```

## 2. 启动流程

正式进程首先验证 `releases/0.7.0` 全树 manifest、source revision、target、`/api/v2`、Schema 和 Web
fingerprint，确认 `STATIC_DIR` 正是该树的 `web/`。随后解析环境，取得数据库锁，对现有 main/WAL/journal
先生成稳定的私有代际快照并在副本上预检，再由 live pool 复核；源 `-shm` 只做文件身份检查，不由 SQLite
打开。然后执行 SQLite integrity/foreign-key 检查并用当前 key 认证全部
持久密文，最后监听。Host credential 必须以 Host ID/`secret` 域 AAD 成功认证；每条 operation request
必须以 operation ID/action/`request_ciphertext` 域 AAD 成功认证、解析为严格当前 enum 且 action 一致。
任何空 AAD 密文、跨行复制、错误用途、发行或状态不一致都发生在网络监听和远端业务执行之前。

## 3. 登录流程

```text
POST /api/v2/auth/login
  -> exact {username,password}
  -> printable ASCII candidate; trim_ascii/lower -> canonical username
  -> source/account admission
  -> bounded Argon2
  -> random Session + CSRF digest in SQLite
  -> Secure Cookie + one-time CSRF plaintext response
  -> unsafe request validates Cookie + CSRF + Origin/Host
```

登出撤销 Session。恢复 Session 时服务端轮换 CSRF 摘要并返回新的 plaintext；CSRF 不写可读 Cookie。
`/api/auth` 或未版本化路径不注册。

username 是 3–64 字节的本地身份，首尾字母/数字且只含 `[a-z0-9._-]`；不是邮箱。配置、CLI、SQLite、
Session、Web 和日志均不保留 `email` 字段或旧名称兼容路径。

浏览器 API 请求经过 Foundation HTTP client，把运行页面的 HTTP(S) origin 作为唯一 `baseUrl`，并再次限制
路径必须位于 `/api/v2/`；unsafe method 才附加本地持有的 CSRF Token。应用层 `AppError` 非 2xx 响应是
严格 `ErrorEnvelope`；Axum JSON/Path/body-limit rejection 当前仍是框架响应，Foundation client 会将其
识别为 `invalid_error_response`，不会宽松接纳。成功 JSON 还必须通过 Session、Host 等端点级 runtime
guard，TypeScript 类型断言本身不构成验证。401 清除本地 CSRF 状态，但 Cookie、Session、登录限流和
Origin/Host 校验仍属于本项目。

## 4. 远端 mutation 状态机

```text
HTTP request + Idempotency-Key
  -> auth/CSRF/strict DTO
  -> HKDF-separated HMAC(request JSON) + HMAC(Idempotency-Key)
  -> allocate operation ID
  -> transaction: request encrypted with operation ID/action AAD + pending operation + requested audit
  -> 202 operation document
  -> per-Host worker claims pending -> running
  -> Sunshine request
       ├─ 可证明成功 -> succeeded
       ├─ 可证明未执行/业务拒绝 -> failed
       └─ 连接中断且副作用不确定 -> unknown
  -> transaction: terminal state + completion outbox
```

Worker 对同一 Host 串行，不同 Host 可并行。调用者断开不取消已经持久化的 operation。状态 API 不返回
actor、原始请求、远端错误正文或凭据。
请求 HMAC 用 constant-time 比较确认“同键同体”，幂等键 HMAC 由 SQLite 按 32-byte BLOB 精确匹配；
二者来自不同 HKDF info，不能互换，也没有裸 SHA-256 读取分支。

## 5. 重启恢复

启动扫描 pending 并继续处理，把上次进程留下的 running 置为 unknown，不猜测向前/向后。audit outbox
按稳定 ID 幂等物化到本地 `audit_logs`；进程或数据库错误后由后台循环继续处理。当前没有外部 audit sink。
只有人工或后续读取能确认 unknown 的远端实际状态。

## 6. 封面代理

```text
external HTTPS cover URL
  -> exact hostname allowlist
  -> DNS: all addresses public
  -> persist encrypted operation
  -> execution-time DNS recheck + pin address set
  -> no redirects, <= 8 MiB, supported image MIME
  -> create 30s one-time internal URL
  -> Sunshine fetches from configured HTTPS proxy origin
  -> peer address + Host/Operation binding authorize response
```

公开 reverse proxy 不应转发一次性内部路径，因为授权依赖真实 transport peer 而非 forwarded headers。

## 7. 管理与维护锁

普通服务持有 maintenance shared。`doctor` 也以 shared 检查；管理员创建/重置取得 exclusive，因此运行
实例存在时失败。未来恢复、转换或 key rotation 若由 `sarmg-upgrade` 实现，也必须取得 exclusive；当前
没有 Sunshine 历史 edge 或 key rotation 流程。固定锁顺序避免同时操作同一 SQLite generation。

## 8. 发行流程

```text
clean checkout + annotated v0.7.0 == HEAD
 -> Web/npm locked build
 -> optimized source-bound Rust binary
 -> exact manifest and archive
 -> staged/re-extracted verification
 -> relocated live asset test
 -> tamper rejection
 -> checksum and immutable publication
```

归档只包含 `0.7.0/` 当前树，没有迁移、备份或恢复逻辑。

Foundation 提供的 current-only Schema identity 会进入 binary identity 的构造，但不会替代本项目更严格的
release-tree verifier。所有构建固定 Foundation 0.3.0：Rust 使用 Git full revision
`1fe326081cfd896f05ff502e80f99504797c14c6`，Web 使用 GitHub Release `v0.3.0` 的四个 tarball 与 lockfile
integrity；不保留 sibling path/file 开发分支。

上述正式构建、binary 和随发行树交付的 Web 仅面向 Linux AMD64。Sunshine Host 与 Moonlight Client 是
控制面管理的外部对象，不属于本仓 Server target，也没有因本项目构建矩阵收窄而改变协议或平台支持。
