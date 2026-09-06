# Sunshine 远端管理

登录后，左侧只显示实例名称。选择实例后，在右侧功能导航中使用：

| 页面 | 功能 |
| --- | --- |
| 连接设置 | 实例名称、地址、端口、Sunshine 凭据；删除管理器中的实例 |
| Sunshine 配置 | 常规、输入、音频/显示、网络、文件路径、编码器分类配置，搜索及完整 JSON 编辑；读取远端语言 |
| 应用管理 | 列出、新增、编辑、删除、关闭当前应用，封面预览和 HTTPS 封面导入 |
| 客户端与配对 | 查看、启用/禁用、解除单个/全部配对，提交 Moonlight PIN |
| 日志 | 按需读取与刷新 Sunshine 日志，按纯文本显示 |
| 服务操作 | 重启 Sunshine、重置显示设备配置 |

配置从远端读取，保留所有实际配置字段，不生成并覆盖远端默认值。分类表单是字段分组，
不是所有操作系统都支持所有字段。未列出的字段也能编辑；高级 JSON 的值必须是字符串，
复杂结构按照 Sunshine 配置格式填写为 JSON 字符串。`status`、`platform`、`version` 是响应元数据，不参与保存。
空值使用 Sunshine 默认值；保存替换配置文件，不自动重启。修改监听地址、端口、证书路径可能断开管理连接。
配置保存和应用序号修改均应避免多个管理员同时编辑；提交前需要核对。

应用表单保留未编辑字段；准备/撤销命令、分离命令以及全部额外字段可在高级 JSON 中修改。
新增使用 index=-1，编辑/删除使用读取时的数组序号。命令在远端执行，不在 Manager 执行。
封面导入仍受服务端 HTTPS 域名允许列表和一次性代理约束，未配置时会拒绝，前端不绕过此策略。

## 异步结果与安全

所有远端修改通过当前管理员 Session/CSRF 和 `/api/v2/sunshine/hosts/{id}/…` API 提交，
每次明确提交生成独立 `Idempotency-Key`。202 只表示进入持久任务队列，不代表成功。
远端 HTTP 2xx 还必须包含布尔 `status: true` 才算成功；`false` 为明确失败，缺失/错误类型为结果不确定。
页面展示 `op_…` 操作 ID 和 pending/running/succeeded/failed/unknown/dead_letter/resolved 状态；
只轮询安全的 GET，每个状态阶段最多 120 次；查询失败后可手动查询，不自动重放修改。

结果不确定或需要人工处理时阻止新的远端修改。先检查远端实际状态，再明确记录成功、失败或无法确认；
人工核对不会重新执行请求。提交响应丢失时显示请求标识，须核对后才可解除页面保护。
操作 ID 可在页面刷新后手动查询，但只允许查询当前管理员自己的任务。
切换实例、刷新或退出不会取消服务端已接受任务。页面草稿和 PIN 不持久化；切换页面前请先处理未保存修改。

认证逻辑不变，401 使会话失效；PIN 提交后立即清空，原始远端异常不展示。
日志是受认证的显式读取内容，可能包含敏感路径，分享前应检查。
管理 Web 的诊断入口和面板已移除，运行健康检查保留。

字段及请求格式核对依据：[Sunshine 配置页面](https://github.com/LizardByte/Sunshine/blob/master/src_assets/common/assets/web/config.html)、
[Sunshine HTTP 实现](https://github.com/LizardByte/Sunshine/blob/master/src/confighttp.cpp)。产品只使用本仓当前 API，不加入历史版本分支。

## 验证

在 `clients/web` 运行 `npm run test:browser`，覆盖 Chromium/Firefox 的远端管理、
失败/不确定状态、严格响应校验、CSRF、401、移动布局及明暗主题 WCAG AA。

已构建 `target/debug/sunshine-manager` 开发二进制后，可运行 `node tests/live-remote-controls.mjs`。
需要 OpenSSL 和 Chromium：测试建立独立 CA/服务端证书、临时数据库和 HTTPS Sunshine 协议夹具，
验证真实 Manager 的配置、应用 CRUD、客户端控制、PIN、日志、重启、显示重置及持久任务查询。
只为测试子进程设置 CA，不修改系统信任库或现有实例；临时证书和数据库结束后清理。
此测试不代表在真实 Sunshine 硬件、编码器或 Moonlight 串流环境中完成验收。
