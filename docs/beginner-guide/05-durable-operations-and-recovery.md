# 05. 持久操作、幂等与恢复

## 5.1 远端不确定性

Sunshine 可能执行请求后在响应前断线。Manager 无法安全断言“超时=未执行”，所以 mutation 不是同步
数据库事务，而是持久 operation 状态机。

## 5.2 状态

`pending` 等待执行；`running` 已由 worker claim；`succeeded` 可证明达到目标；`failed` 可证明拒绝/未执行；
`unknown` 无法证明副作用。终态不会因页面刷新而消失。

## 5.3 幂等键

同 actor/Host/action/key 和规范请求 Hash 决定唯一 operation。完全相同请求返回原 operation；不同请求
复用 key 返回 409。客户端应在一次用户意图的网络重试中保留 key。

## 5.4 Per-Host 串行

同一 Sunshine Host 的 mutation 按队列顺序执行，避免应用创建/删除或客户端操作乱序；不同 Host 在全局
预算内并行。慢 Host 不应占满所有 worker。

## 5.5 Worker 流程

claim pending，解密并再次验证请求/credential，构造有界远端调用，根据可证明结果分类，再用事务写终态
和 completion outbox。外部调用期间不持有 SQLite 写事务。

## 5.6 重启

启动扫描 pending 继续；遗留 running 一律转 unknown，因为旧进程可能已完成远端调用。禁止启动时自动
重放 running 来追求“最终成功”。

## 5.7 Outbox

审计事件与业务事务同时写入 outbox，投递失败可按稳定 ID 重试。Secret 和远端正文先做最小安全投影，
不能因为 outbox 受保护就保存不必要敏感数据。

## 5.8 Unknown 处置

停止同一 Host 的盲目写入，读取 Sunshine actual state、日志和 operation 摘要。根据当前事实发起新的
明确意图；不要编辑数据库或使用新 key 重复原动作来“碰碰运气”。

## 5.9 测试矩阵

覆盖调用前失败、明确 4xx、5xx、响应体非法、响应前断线、完成事务失败、进程 kill、重启、同 key 并发、
不同 Host 公平性和 outbox 重投。
