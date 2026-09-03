# 04. 登录、Session 与请求生命周期

## 4.1 登录准入

```text
TCP peer -> Foundation Axum adapter -> username candidate -> source/account budgets
 -> Foundation bounded Argon2 -> random Session/CSRF -> digest in SQLite
```

登录 JSON 精确为 `{username,password}`。username 候选限 1–64 个可打印 ASCII 字节；Server 只做
`trim_ascii()` 和 ASCII 小写化，结果必须为 3–64 字节、首尾字母/数字、中间仅 `[a-z0-9._-]`。它是本地管理员
用户名，不是邮箱，`@` 和 Unicode 均拒绝；持久行、Session 和限流键只保存 canonical 结果。未知账户执行
相同参数 Hash，降低枚举。并发和总耗时有上限，避免密码验证耗尽 CPU。

## 4.2 Session

浏览器只持有 HttpOnly 随机 Session Cookie，并把登录或恢复响应中的 CSRF plaintext 保留在内存；SQLite
Session 表只保存两个 token 的摘要、管理员 user ID、创建/最后使用和撤销状态，canonical username 来自
关联的 `_sarmg_administrators` 行。恢复 Session
响应精确包含 `{authenticated,user_id,username,role,csrf_token}`，并生成新的 CSRF token、替换
服务端摘要，被替换 token 立即失效。空闲与绝对 TTL 同时生效；登出在 Server 端撤销。

## 4.3 Unsafe 请求

写请求必须同时满足有效 Session、匹配 CSRF、可信 Origin/Host 和严格 Content-Type/body。Cookie 自动
附带并不等于请求被授权。读请求也受数据投影和会话范围限制。

## 4.4 来源事实

当前只以 transport peer 作为来源，不读取 `Forwarded`、`X-Forwarded-For` 等 header，也没有 trusted-proxy
CIDR 配置。经反向代理登录时，Manager 的来源预算看到的是代理地址，因此代理层还需独立限制真实客户端；
一次性封面内部路径则不得经会隐藏 peer 的公共代理。未来若支持 forwarded header，必须先增加显式可信
代理边界，不能让公网调用方自报 IP。

## 4.5 读取 Host

Host 列表读取认证后从 SQLite 取得受限投影和内存探测快照；应用、客户端、配置、日志与封面读取才会按
各自 route 同步调用有界 Sunshine client。远端失败应分类并隐藏敏感正文，不能把数据库 credential 发给
浏览器。

## 4.6 写入请求

远端 mutation 验证 `Idempotency-Key` 和 DTO，以独立 HKDF 域 HMAC 请求 JSON 和 Idempotency-Key，
再在短事务中写 encrypted request、pending operation
和审计。返回 `202` 只表示 durable accept；执行结果通过 operation API 获取。

## 4.7 常见状态码

401 会话无效；403 CSRF/Origin/管理员安全检查失败；409 幂等冲突或 operation 状态不允许；422 是 JSON
提取/形状错误；429 准入受限；
503 worker/数据库等暂不可用。客户端重试策略必须依据分类，而不是所有非 2xx 都循环。

## 4.8 安全日志

记录 request/operation/Host 的受限标识和错误类别。登录密码、Cookie、CSRF、Sunshine credential、完整
URL、encrypted request 和 upstream body 不记录。

## 4.9 路由变更

当前只注册 `/api/v2/auth/*` 与 `/api/v2/sunshine/*`。破坏性变更同步更新 Web、测试和发行身份，删除
被替代路由，不加 redirect 或 alias。
