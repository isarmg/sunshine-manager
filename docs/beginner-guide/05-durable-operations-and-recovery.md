# 05. 持久操作、幂等与恢复

## 5.1 远端不确定性

Sunshine 可能执行请求后在响应前断线。Manager 无法安全断言“超时=未执行”，所以 mutation 不是同步
数据库事务，而是持久 operation 状态机。

## 5.2 状态

`pending` 等待执行；`running` 已由 worker claim；`succeeded` 可证明达到目标；`failed` 可证明拒绝/未执行；
`unknown` 无法证明副作用且永不重放；`dead_letter` 表示重试预算耗尽或无法确认的操作已封存；`resolved`
只追加管理员核验结论。终态不会因页面刷新而消失。

## 5.3 幂等键

同 actor/Host/action/key 和规范请求 fingerprint 决定唯一 operation。完全相同请求返回原 operation；不同请求
复用 key 返回 409。这里的两个数据库摘要并非裸 Hash：credential master key 经 HKDF-SHA-256 使用独立
info 派生 request-fingerprint key 与 idempotency-key-hash key，再分别计算 HMAC-SHA-256。请求 fingerprint
用 constant-time 比较，Idempotency-Key HMAC 作为 SQLite 精确 BLOB 查找键；不尝试裸 SHA-256 fallback。
客户端应在一次用户意图的网络重试中保留 key。

## 5.4 Per-Host 串行

同一 Sunshine Host 的 mutation 按队列顺序执行，避免应用创建/删除或客户端操作乱序；不同 Host 在全局
预算内并行。慢 Host 不应占满所有 worker。

## 5.5 Worker 流程

claim pending，以 operation ID/action/字段域 AAD 认证解密 request，再以 Host ID/字段域 AAD 认证解密
credential，把请求解析为严格当前 enum 并核对持久 action；任何跨 operation、跨 action 或跨 Host 密文
调换都会在远端调用前成为 `request_corrupt` 或本地状态不可用。封面 URL 还会在执行期重新执行网络策略。
其他字段边界依赖入队前验证与严格 enum，不宣称 worker 重复所有 HTTP 校验。
随后构造有界远端调用，根据可证明结果分类，再用事务写终态和 completion outbox。外部调用期间不持有
SQLite 写事务。

## 5.6 重启

启动扫描 pending 继续；中断留下的 running 一律转 unknown，因为退出前可能已完成远端调用。禁止启动时自动
重放 running 来追求“最终成功”。

## 5.7 Outbox

审计事件与业务事务同时写入 outbox，再按稳定 ID 幂等物化到同一 SQLite 的 `audit_logs`。进程或数据库
错误后后台循环继续处理；当前没有外部 audit sink。Secret 和远端正文先做最小安全投影，不能因为 outbox
受保护就保存不必要敏感数据。

## 5.8 Unknown 处置

停止同一 Host 的盲目写入，读取 Sunshine actual state、日志和 operation 摘要。通过 resolve 接口记录
人工确认结果，解除该 Host 的不确定状态后，才可根据当前事实发起新的明确意图。
不存在将终态或 unknown 重排回 pending 的 retry 接口；不要编辑数据库或换 key 盲目重放。

## 5.9 测试矩阵

覆盖调用前失败、明确 4xx、5xx、响应体非法、响应前断线、完成事务失败、进程 kill、重启、同 key 并发、
不同 Host 公平性和 outbox 重投。
