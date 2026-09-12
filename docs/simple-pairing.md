# 简化配对（Server 0.10.4 / Client 0.1.0-rc.7）

客户端 UI 仅填写 Server 地址、256-bit 配对码、本机 Sunshine 回环地址/端口及账号密码。
CA 文件、Manager ID、设备 ID 不再是用户输入项。客户端与 Server 需同时部署支持本流程的版本。

`POST /sunshine-client/v1/pairing` 接受唯一字段 `token`，返回 `manager_id`、`device_id`。
沿用 Client 的可信 HTTPS 入口限制、无 Cookie/Origin、16 KiB 请求上限和 `Cache-Control: no-store`。
查询不消费配对码；仅匹配未注册、未撤销且未取消的哈希记录。错误令牌统一拒绝，不回显令牌。
真正注册仍通过 `/sunshine-client/v1/enroll` 的原子单次消费完成，查询后发生取消/撤销会使注册失败。
客户端在注册前持久化独立随机凭据，回执丢失仍使用原有 identity 查询恢复，不创建另一套任务状态机。

仅使用系统信任库，不自动接纳自签名证书，不关闭 TLS 校验，不跟随重定向，不将凭据写入日志。
Windows LocalSystem 服务使用计算机信任上下文。Sunshine 默认证书若不受信任或名称不匹配，必须先由管理员完成合法证书部署，配对不能绕过这个条件。

Windows Client 最低 Windows 11 x86_64/build 22000。正常菜单退出会先停止服务，停止失败则保留托盘；服务不再无条件随开机自动运行。
Sunshine 重启与退出 Client 是两个操作；Client 仍独立于 Sunshine 进程，不修改视频串流链路。
