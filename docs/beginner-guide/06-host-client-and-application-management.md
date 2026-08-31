# 06. Sunshine Host、客户端与应用管理

## 6.1 Host 记录

Host 保存展示身份、规范 endpoint、TLS/连接策略和加密 credential。新增/修改时先验证 URL scheme、Host、
端口和边界；不能允许 URL 用户信息把密码混入日志或错误。

## 6.2 TLS

所有 Sunshine 连接固定使用 HTTPS，并由平台信任库强制验证证书链、有效期与主机名。API、数据库和 Web
均不存在关闭校验的字段；自签名证书必须先按操作系统安全流程加入受信任根，不能通过产品开关绕过。

## 6.3 客户端管理

Manager 查询 Sunshine 已配对客户端并通过持久 operation 执行支持的管理动作。列表是读取时快照；在
用户确认与执行之间可能变化，因此 mutation 需要稳定远端 ID/当前 revision，而非仅用显示名。

## 6.4 应用管理

应用 DTO 包含 Sunshine 支持的命令、工作目录、图像等字段。输入必须限制长度、集合和控制字符；Manager
不是通用远程 Shell UI，不额外扩展任意 Host 命令能力。

## 6.5 删除语义

删除请求持久化后由 worker 调用远端。`succeeded` 表示远端响应可证明目标状态；网络断线可能 unknown。
UI 不能提前从列表永久移除项目而掩盖未知结果。

## 6.6 并发编辑

资源 revision 与 Idempotency-Key 分别解决“基于过期页面覆盖”和“同一意图网络重试”。二者不能相互
替代。冲突应要求刷新并重新确认。

## 6.7 Credential 生命周期

Secret 进入请求后立即按当前 envelope 加密；解密只在连接使用期间。修改 Host credential 是新的持久
操作/状态变更，并写审计摘要，但不记录旧值或新值。

## 6.8 故障定位

先区分 DNS、TCP/TLS、认证、Sunshine API 版本/响应、业务拒绝和 Manager operation。不要用 `curl -k`
成功作为应用 TLS 配置正确的证明。

## 6.9 能力边界

具体客户端/应用字段受当前 Sunshine API 和仓库测试约束；文档未列出的远端接口不自动成为支持能力。
