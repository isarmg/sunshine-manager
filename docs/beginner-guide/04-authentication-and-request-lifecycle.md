# 04. 登录、Session 与请求生命周期

## 4.1 登录准入

```text
TCP peer -> body limit -> source/account/global budgets
 -> bounded Argon2 -> random Session/CSRF -> digest in SQLite
```

未知账户执行相同参数 Hash，降低枚举。并发和总耗时有上限，避免密码验证耗尽 CPU。

## 4.2 Session

浏览器只持有随机 Cookie 与一次性返回的 CSRF plaintext，SQLite 保存摘要、actor、创建/最后使用和撤销
状态。空闲与绝对 TTL 同时生效；登出在 Server 端撤销。

## 4.3 Unsafe 请求

写请求必须同时满足有效 Session、匹配 CSRF、可信 Origin/Host 和严格 Content-Type/body。Cookie 自动
附带并不等于请求被授权。读请求也受数据投影和会话范围限制。

## 4.4 来源事实

默认以 transport peer 作为来源。只有明确部署并验证可信 proxy 时，才按当前配置解释 forwarded header；
不能让公网调用方自报 IP 绕过登录限流或封面 peer 绑定。

## 4.5 读取 Host

读请求认证后从 SQLite 取得受限 Host 投影，再通过有界 Sunshine client 查询 actual state。远端失败应
分类并隐藏敏感正文，不能把数据库 credential 发给浏览器。

## 4.6 写入请求

写请求验证 `Idempotency-Key`、资源 revision 和 DTO，在短事务中写 encrypted request、pending operation
和审计。返回 `202` 只表示 durable accept；执行结果通过 operation API 获取。

## 4.7 常见状态码

401 会话无效；403 CSRF/Origin/授权失败；409 幂等或 revision 冲突；422 参数语义错误；429 准入受限；
503 worker/数据库等暂不可用。客户端重试策略必须依据分类，而不是所有非 2xx 都循环。

## 4.8 安全日志

记录 request/operation/Host 的受限标识和错误类别。登录密码、Cookie、CSRF、Sunshine credential、完整
URL、encrypted request 和 upstream body 不记录。

## 4.9 路由变更

当前只注册 `/api/v2/auth/*` 与 `/api/v2/sunshine/*`。破坏性变更同步更新 Web、测试和发行身份，删除
被替代路由，不加 redirect 或 alias。
