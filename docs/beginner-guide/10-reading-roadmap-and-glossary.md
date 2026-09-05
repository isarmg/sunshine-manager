# 10. 源码路线、练习与术语表

## 10.1 阅读路线

先看数据库/credential/operation 类型，再读 auth 与 routes，然后读 Sunshine client、worker、cover
policy/proxy，最后看 Web、release 和部署。先理解权威状态，再理解页面。

## 10.2 按问题找入口

| 问题 | 入口 |
|---|---|
| 登录被拒绝 | auth、login admission、Session/CSRF |
| 远端操作卡住 | operations、worker、Sunshine client |
| 重启后 unknown | operation startup recovery |
| 封面失败 | cover policy、DNS、proxy token |
| 启动拒绝 | release、database schema、credentials/locks |
| Web 与实际不一致 | auth phase、Host overview fetch；operation 当前需从 API 单独查询 |

## 10.3 练习

1. 临时环境登录并读取测试 Host。
2. 用同一幂等键重试，证明 operation ID 不变。
3. 模拟响应丢失并观察 unknown。
4. 让 DNS 返回公网+内网地址，确认封面拒绝。
5. 篡改复制 release 的 Web asset，确认 verify 失败。
6. 证明数据库与错误 external key 组合会被 doctor 拒绝，并记录当前没有受支持 restore 流程。

## 10.4 术语

| 术语 | 含义 |
|---|---|
| Host | 一个受管 Sunshine 实例及其连接身份 |
| mutation | 会改变远端状态的请求 |
| operation | 持久化、可查询的远端变更意图 |
| Idempotency-Key | 把网络重试绑定到同一意图的调用方键 |
| unknown | 无法证明远端副作用的终态 |
| outbox | 与业务事务一起记录的待处理审计事件 |
| SSRF | 服务端被诱导访问敏感网络目标 |
| DNS rebinding | 同一名称在校验与执行时解析为不同目标 |
| pin | 把执行连接限制到校验通过的完整地址集合 |
| credential envelope | `sunshine:sgev1:` 存储结构；只有在 external key 和记录身份/字段域 AAD 下认证成功才是当前密文 |
| source-bound | binary 身份绑定源码 revision |
| maintenance lock | 产品与离线工具协调数据库访问的锁 |

## 10.5 学成标准

应能解释 202/unknown、幂等键为何不等于 Host 并发控制、为什么远端调用不在 SQLite 事务、封面为何需
提交前校验并在执行期重新解析、external key 为什么独立保管、当前 Schema 为什么不现场转换。

## 10.6 深入入口

完整时序见[工作流程](../project-workflow.md)，能力边界见[功能与取舍](../feature-inventory-and-tradeoffs.md)，
生产部署、当前连续性限制和事件处置见[运维文档](../operations.md)。
