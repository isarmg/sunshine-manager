# 06. Sunshine Host、客户端与应用管理

## 6.1 Host 记录

Host 保存展示身份、规范 host/port 和加密 credential。URL scheme 不是输入字段：Manager 总是由 host 与
port 生成 HTTPS endpoint。新增/修改时验证 DNS/IP、端口和文本边界；不能允许 URL 用户信息进入模型。

## 6.2 TLS

所有 Sunshine 连接固定使用 HTTPS，并由平台信任库强制验证证书链、有效期与主机名。API、数据库和 Web
均不存在关闭校验的字段；自签名证书必须先按操作系统安全流程加入受信任根，不能通过产品开关绕过。

## 6.3 客户端管理

Manager 查询 Sunshine 已配对客户端并通过持久 operation 执行支持的管理动作。列表是读取时快照；在
用户确认与执行之间可能变化，因此 mutation 使用稳定远端 UUID，而非仅用显示名。当前 Host CRUD 没有
revision/`If-Match`；两个页面同时修改 Host 时是后写覆盖先写，管理员应在变更前刷新。

## 6.4 应用管理

应用保存请求当前把 Sunshine application JSON 作为不透明顶层 object 传递，只限制序列化后不超过
256 KiB；Manager 没有逐字段复制 Sunshine 的 command、working directory 或 image Schema。应用列表仅
要求 `apps` 为最多 512 个 object。这个取舍避免维护另一份上游 Schema，但字段语义和命令风险仍由当前
Sunshine API 决定；Manager 也没有额外的 Host OS 任意命令 route。

## 6.5 删除语义

删除请求持久化后由 worker 调用远端。`succeeded` 表示远端响应可证明目标状态；网络断线可能 unknown。
UI 不能提前从列表永久移除项目而掩盖未知结果。

## 6.6 并发编辑

`Idempotency-Key` 只解决同一远端意图的网络重试，不防止过期页面覆盖 Host 配置。Host mutation 在单进程
内按 Host 串行，但这不等于乐观并发控制。若未来增加 revision，必须同时定义数据库 compare-and-swap、
HTTP precondition、409、Web 刷新和并发测试；当前不能依赖该能力。

## 6.7 Credential 生命周期

Host credential 在写入数据库前按当前 envelope 加密，读取 Host 响应只暴露 `password_set`。修改 Host
记录是本地数据库事务，并非 durable remote operation；同一事务会写不含 Secret 的 `audit_logs`。Host
CRUD 没有 `audit_outbox` 外部投递闭包，因此需要集中审计 sink 的环境仍应在代理/调用侧补充管理事件。

## 6.8 故障定位

先区分 DNS、TCP/TLS、认证、Sunshine API 版本/响应、业务拒绝和 Manager operation。不要用 `curl -k`
成功作为应用 TLS 配置正确的证明。

## 6.9 能力边界

具体客户端/应用字段受当前 Sunshine API 和仓库测试约束；文档未列出的远端接口不自动成为支持能力。
