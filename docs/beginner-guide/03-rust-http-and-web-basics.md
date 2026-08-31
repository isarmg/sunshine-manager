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

## 3.4 Web 状态

React 页面保存交互状态和 Server 投影，不是权威数据库。401 时清理当前会话，409 呈现冲突，202 后
轮询原 operation，unknown 要显示人工核对提示，不能自动再发新请求。

## 3.5 安全整数与时间

跨 JSON 数值必须处于 JavaScript 可精确范围或使用明确字符串编码。所有持久时间使用 UTC 规范形式，
超时/退避用单调时钟；浏览器本地时间只用于展示。

## 3.6 错误 envelope

客户端依据 HTTP status 与稳定 code 分支，message 仅供展示。上游 Sunshine 正文、URL credential、密文、
内部 SQL 和栈不得放入 API error。request ID 用于关联受保护日志。

## 3.7 数据库事务边界

创建 operation、加密请求和 requested audit/outbox 属于同一短事务；远端 HTTP 不在事务内。终态和 completion
outbox 再使用独立事务，避免持锁等待网络。

## 3.8 修改建议

先画出 Browser -> route -> transaction -> worker -> Sunshine -> terminal -> Web 的完整链路，再改字段。
任何一端“暂时接受两种形状”都会扩大当前合同和测试矩阵。

## 3.9 练习

跟踪一个应用重命名请求，记录 request ID、Idempotency-Key、operation ID、encrypted request、Host 串行
键、Sunshine 调用和 Web 终态展示。
