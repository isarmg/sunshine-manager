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
服务端不解析另一种旧封面字段。

## 7.7 Schema/密文合同

当前数据库由 code-owned DDL fingerprint 与 metadata 双验证；credential 只接受当前 external key/envelope。
封面 operation request 也属于需认证的持久密文，不能忽略 pending/running 行。

## 7.8 测试负例

覆盖 DNS 多地址、重绑定、IPv4-mapped IPv6、redirect、超大 Content-Length/stream、MIME 欺骗、慢响应、
一次性重放、过期、错误 Host/operation 和 forwarded peer spoof。

## 7.9 变更原则

封面能力扩展必须重新评估 threat model、proxy 拓扑和 egress；不能仅添加一个 URL scheme/redirect 开关。
