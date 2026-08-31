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
├─ 浏览器
│  ├─ login -> Session + CSRF
│  ├─ read Hosts/clients/apps
│  └─ mutation + Idempotency-Key -> operation
├─ Worker
│  ├─ per-Host serialization
│  ├─ decrypt request/credential
│  ├─ call Sunshine
│  └─ succeeded/failed/unknown + audit
└─ 运维
   ├─ identity/verify-release/doctor
   └─ sarmg-upgrade backup/restore/version/key operations
```

## 2. 启动流程

正式进程首先验证 `releases/0.7.0` 全树 manifest、source revision、target、`/api/v2`、Schema 和 Web
fingerprint，确认 `STATIC_DIR` 正是该树的 `web/`。随后解析环境，取得数据库锁，在私有副本验证现有
Schema，解密检查持久 Secret，最后监听。任何发行或状态不一致都发生在网络监听和业务写入之前。

## 3. 登录流程

```text
POST /api/v2/auth/login
  -> body/source/account admission
  -> bounded Argon2
  -> random Session + CSRF digest in SQLite
  -> Secure Cookie + one-time CSRF plaintext response
  -> unsafe request validates Cookie + CSRF + Origin/Host
```

登出撤销 Session。旧 `/api/auth` 或未版本化路径不注册。

## 4. 远端 mutation 状态机

```text
HTTP request + Idempotency-Key
  -> auth/CSRF/strict DTO
  -> transaction: encrypted request + pending operation + requested audit
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

## 5. 重启恢复

启动扫描 pending 并继续处理，把上次进程留下的 running 置为 unknown，不猜测向前/向后。audit outbox
按稳定幂等 ID 重投，不因进程中断丢失。只有人工或后续读取能确认 unknown 的远端实际状态。

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

普通服务持有 maintenance shared。`doctor` 也以 shared 检查；管理员创建/重置、恢复、升级和 key rotation
取得 exclusive，因此在运行实例存在时失败。固定顺序避免同时操作同一 SQLite generation。

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
