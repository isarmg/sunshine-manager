# sunshine-agent（开发中）

这是 Sunshine Manager 产品内的独立 Agent 执行内核，**尚不是可安装运行的 Agent 发布包**。
目前实现协议、白名单、本机 HTTPS 适配、串行执行、去重/核对、Linux 安全执行日志及 WSS 客户端传输。
注册/Manager 接入、凭据落盘、系统服务和 Windows 安全日志仍待完成。

目标：Windows x86_64、Linux x86_64；固定适配 Sunshine 官方 v2026.516.143833。
它不处理 Sunshine–Moonlight 串流，不提供视频转发、编码、脚本或通用远程控制。

实施边界、协议语义、验证命令和未完成项见 [Agent 管理实施记录](../docs/agent-management-v1.md)。
