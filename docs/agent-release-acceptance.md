# Agent 0.1.0-rc.1 验收记录

记录日期：2026-09-07 UTC。客户端源码与标签绑定：
`428a538cffcba55341fcd7dbcb78ce3e97bbbff7` / `agent-v0.1.0-rc.1`。

## 原生构建与安装

[精确提交的双平台检查](https://github.com/isarmg/sunshine-manager/actions/runs/34084295438) 已通过。
[同一提交的主 CI](https://github.com/isarmg/sunshine-manager/actions/runs/34084274813) 已通过。

| 环境 | 已通过的范围 |
| --- | --- |
| Ubuntu 24.04 x86_64 | Agent/协议测试、Clippy、HTTPS/WSS、原生优化构建、独立包/身份校验、systemd 安装/启动/重启、拒绝覆盖、卸载保留身份与去重状态 |
| Windows Server 2025 x86_64 | 同上；使用原生 MSVC 静态 CRT、Windows Service、ACL 与 SQLite 持久状态；不是交叉编译或服务模拟 |
| Windows 11 Pro 10.0.26200 x64 | 八项归档/PowerShell 回归测试、下载包独立校验和实际二进制身份校验；下述真实 Sunshine 前台 Agent 闭环 |

安装检查使用一次性 GitHub-hosted runner，不在用户主机注册测试服务。
服务自动启动配置已验证，没有把 runner 重启或真实桌面服务部署当作已完成事实。

## 真实 Sunshine 闭环

Windows 11 上另建官方便携版 `v2026.516.143833`，ZIP SHA-256：
`0a3af3dde43b8f2c94ffe04b850ad736d6e1be2b75906779d7094a5ad9d4783b`。
使用独立私有目录、账户、CA/证书、空应用列表及回环管理端口 48990。
没有替换现有 `2026.528.35537` 安装、配置、证书或凭据；没有操作原有系统服务。
Manager 使用独立临时数据库和当前 0.9.1 开发测试入口，真实 Chromium 页面及 TLS/WSS 入口。
Windows Agent 使用上述已验证发行候选二进制，以当前用户前台进程运行，不冒充本机服务安装验收。

实测通过：

1. 设备通过严格验证的 HTTPS 注册及 WSS 建连，Agent 经严格验证的回环 HTTPS 读取官方 Sunshine。
2. 网页预览并保存名称，逐字段核对所有未修改配置仍保留；响应元数据没有写入配置文件；状态为等待重启。
3. 旧配置修订拒绝执行；相同幂等键重复提交返回相同持久操作。
4. 停止并重启独立 Agent 后保留注册身份，完成离线时提交的任务。
5. 管理员明确提交隔离 Sunshine 重启；Agent 通道保持在线，Sunshine 随后恢复 HTTPS 可达且读回保存值。
6. 上游重启未返回可验证确认，结果正确为 `unknown / restart_not_confirmed`，`attempt=1`。
   没有因为接口恢复可达就将其改成成功，也没有将配置读回冒充运行时生效证明。
7. 撤销凭据后实际 Agent 断开；持久操作审计和设备撤销审计存在。
8. Manager/Agent 日志不包含本次 Sunshine 测试密码或配对码，浏览器没有运行时错误。

Manager 与 Agent 测试进程在结束时停止；独立 Sunshine 测试进程另外按精确可执行路径停止。
原始私密测试数据仅本地保留，未提交或上传。此文只记录脱敏结果。

## 未宣称完成的项目

- Linux 真实 Sunshine 主机配置/重启闭环尚未验收；systemd 安装通过不等于该项完成。
- 没有验收 Moonlight 串流、编码质量、全部显卡/驱动、所有 Linux 发行版或旧 Sunshine 版本。
- Windows 包没有 Authenticode 签名。校验和及源码身份不代替发布者代码签名。
- 真实 `unknown` 场景的强制重复投递/崩溃注入矩阵由执行内核和协议夹具测试覆盖，不能改称全部在真实 Sunshine 上重演。

因此本次是独立客户端候选版，不把上述边界省略后宣称双平台稳定版验收完成。
最终附件应来自[候选标签检查](https://github.com/isarmg/sunshine-manager/actions/runs/34084882407)的成功作业，
发布前再次核验源码身份及校验和；不修改 Manager 的既有 Release。
