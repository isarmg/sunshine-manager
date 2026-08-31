# 03. Rust、HTTP 与 Web 基础

## 3.1 Rust 领域类型

Host、application、client、operation、Session 和 credential envelope 使用明确类型与严格枚举。外部
JSON 是不可信输入，必须在进入数据库或 Sunshine client 前完成长度、格式、集合和语义校验。

## 3.2 异步所有权

HTTP handler 只负责认证、验证和持久化意图；worker claim operation 后拥有远端执行责任；完成事务记录
终态。调用者断开不会撤销已经持久化的 operation。

## 3.3 有界 HTTP

浏览器请求和 Sunshine 响应均需 body 大小、连接/总超时、并发与错误正文上限。远端返回成功状态也不
意味着 JSON 符合合同；解析和业务验证仍要执行。

浏览器侧复用 Foundation 0.3 的 same-origin HTTP client 与错误合同：`baseUrl` 固定为当前页面 origin，
调用路径还必须位于 `/api/v2/`。每个成功响应传入明确 runtime guard；不能用 `as T` 把未知 JSON 当成
已验证对象。到 Sunshine Host 的 HTTPS client 则有产品特有的 TLS、DNS、redirect 和正文策略，仍在本项目。

## 3.4 Web 状态

React 页面保存交互状态和 Server 投影，不是权威数据库。当前页面只实现认证状态机与 Host 只读概览，
认证丢失时清空 Host 和错误状态；尚未实现 409 展示、202 operation 轮询或 unknown 处置 UI。直接调用
管理 API 的客户端必须自行保留 Idempotency-Key、轮询原 operation，并在 unknown 时要求人工核对。

## 3.5 安全整数与时间

跨 JSON 数值必须处于 JavaScript 可精确范围或使用明确字符串编码。所有持久时间使用 UTC 规范形式，
超时/退避用单调时钟；浏览器本地时间只用于展示。

## 3.6 错误 envelope

服务端 `AppError` 通过 Foundation `sarmg-error` 只输出当前严格形状
`{code,message,retryable,request_id?,details?}`；客户端依据 HTTP status 与稳定 code 分支，message 仅供
展示。未知字段、缺少 `retryable` 或非当前 `{code,message}` 都必须拒绝。上游 Sunshine 正文、URL credential、
密文、内部 SQL 和栈不得放入 API error；可选 request ID 只用于关联受保护日志。

## 3.7 数据库事务边界

创建 operation、加密请求和 requested audit/outbox 属于同一短事务；远端 HTTP 不在事务内。终态和 completion
outbox 再使用独立事务，避免持锁等待网络。

## 3.8 修改建议

先画出 Browser -> route -> transaction -> worker -> Sunshine -> terminal -> Web 的完整链路，再改字段。
任何一端“暂时接受两种形状”都会扩大当前合同和测试矩阵。

## 3.9 练习

通过受控 API 调用跟踪一个应用保存请求，记录 Idempotency-Key、operation ID、encrypted request、Host
串行键、Sunshine 调用和终态；然后确认当前内置 Web 不会虚构该 operation 的展示或成功状态。
