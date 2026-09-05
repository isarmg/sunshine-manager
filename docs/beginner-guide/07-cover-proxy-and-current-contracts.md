# 07. 封面代理、SSRF 与当前协议

## 7.1 为什么封面危险

用户提供 URL 后由服务器或 Sunshine 获取内容，攻击者可能诱导访问 loopback、内网、云 metadata，或用
DNS rebinding/redirect 改变目标。这是 SSRF 边界，不是普通图片下载。

## 7.2 准入流程

只允许 HTTPS；hostname 必须精确位于 allowlist；DNS 所有解析地址均须为公网；禁止 URL credential、
危险端口和合同外结构。仅有一个公网地址而另一个为内网也必须拒绝。

## 7.3 执行时复验

operation 执行时重新解析并固定完整地址集合，防止创建后 DNS 变化。请求禁止 redirect，限制连接/总
超时、8 MiB 和支持的图片 MIME；状态码和正文都严格处理。

## 7.4 一次性内部 URL

Sunshine 得到的不是原 URL，而是 30 秒有效、绑定 Host/operation/来源地址的一次性 HTTPS URL。代理
路径依赖真实 transport peer，不应经过会隐藏/伪造 peer 的公共转发链。

## 7.5 Egress

应用校验是纵深防御，不能替代操作系统/网络 egress firewall。生产还应拒绝 private、loopback、link-
local、multicast 和 metadata 段，并限制 DNS resolver 与 Sunshine Host 的访问范围。

## 7.6 API 合同

当前 DTO 严格拒绝 unknown fields、非法枚举、超长 URL/文本和非规范 ID。Web 必须发送当前 `/api/v2`，
服务端不解析其他封面字段形状。

## 7.7 Schema/密文合同

当前数据库由 code-owned DDL fingerprint 与 metadata 双验证；credential 只接受当前 external key/envelope。
当前 envelope 文本为 `sunshine:sgev1:<key-id>:<base64(SGEV envelope)>`，但 GCM 认证输入不只有密文：
代码用 `sunshine-manager:aes-256-gcm:aad:v1` 作为 AAD 格式域，并对每个组件使用 64-bit big-endian 长度分帧。
Host credential 的组件为用途域 `host-credential`、Host ID、字段 `secret`；operation request 的组件为用途域
`operation-request`、operation ID、action、字段 `request_ciphertext`。AAD 不另存数据库，而是由当前行身份
确定性重建。空 AAD、错误字段域、行间复制、action 被改写或旧实现生成的同前缀密文一律认证失败；没有
fallback。封面 operation request 也属于需认证的持久密文，启动必须扫描所有状态而非只看 pending/running。

同一 external master key 还以固定 salt `sunshine-manager:credential-master-key:hkdf-sha256:v1` 进入
HKDF-SHA-256。request fingerprint 的 info 是
`sunshine-manager:operation-request-fingerprint:hmac-sha256:v1`，Idempotency-Key 数据库 hash 的 info 是
`sunshine-manager:operation-idempotency-key-hash:hmac-sha256:v1`；每个输出是独立的 32-byte HMAC-SHA-256
key。两个持久值仍为 32-byte BLOB，因此 DDL/SHA 不变；裸 SHA-256、另一域的 HMAC 或另一 master key
生成的值均不是当前合同。启动可用已解密 request 重算并认证 fingerprint；Idempotency-Key 本身不持久化，
其 HMAC 只作为 `(actor,host,action,key)` 唯一索引中的精确 BLOB。

## 7.8 测试负例

覆盖 DNS 多地址、重绑定、IPv4-mapped IPv6、redirect、超大 Content-Length/stream、MIME 欺骗、慢响应、
一次性重放、过期、错误 Host/operation 和 forwarded peer spoof。
密文合同还要覆盖篡改、空 AAD、跨 Host/operation/action/字段调换；HMAC 合同覆盖同值稳定、跨域不同、
换 master key 不同、低熵输入不等于裸 SHA-256、非 32-byte 拒绝和启动全量复算。

## 7.9 变更原则

封面能力扩展必须重新评估 threat model、proxy 拓扑和 egress；不能仅添加一个 URL scheme/redirect 开关。
