# 02. 开发环境与第一次运行

## 2.1 工具链

仓库固定 Rust `1.98.0`，Web 使用 lockfile 对应 Node/npm。先建立不修改锁图的基线：

```bash
cargo +1.98.0 check --locked --all-targets
cd clients/web
npm ci
npm run build
```

## 2.2 临时依赖

准备仅当前用户可访问的临时 SQLite 目录、已构建 Web `dist`、开发管理员密码、随机 32 字节 Base64
credential key 与明确 key ID。可使用测试 Sunshine 或仓库 mock，不要指向生产 Host。

## 2.3 开发启动

按当前配置解析器设置 `SUNSHINE_MANAGER_*`，显式打开开发模式并绑定回环：

```bash
cargo +1.98.0 run -- serve
```

正式 source-bound binary 拒绝普通 `serve`，只能以 `serve-release --root <exact-release>` 启动。

## 2.4 第一次浏览器流程

登录后检查 Session/CSRF，再添加一个测试 Host。读取请求可以直接查询 Sunshine；写请求应返回 operation。
轮询 operation 到 `succeeded`、`failed` 或 `unknown`，不要以页面按钮消失作为成功证据。

## 2.5 第一次封面练习

默认 allowlist 为空。要测试远程封面，应建立可控的公网 DNS/HTTPS fixture，配置精确 hostname 与内部
HTTPS proxy origin，验证 redirect、private IP、错误 MIME 和超大响应均被拒绝。

## 2.6 成功标准

- Server 只监听回环；
- 新当前数据库和全部密文通过 doctor；
- 登录 Cookie/CSRF 生效；
- 同幂等键同请求返回同 operation；
- 重启后 pending 继续、running 转 unknown；
- Web build 和 Rust test 基线通过。

## 2.7 常见失败

| 现象 | 检查 |
|---|---|
| 启动拒绝 | DATABASE_URL、STATIC_DIR、key、Schema、instance lock |
| Cookie 不回传 | HTTPS、Secure、Origin/Host、proxy header |
| Host 连接失败 | URL、TLS CA、认证、超时、Sunshine API |
| operation pending | worker readiness、per-Host 队列 |
| 封面拒绝 | allowlist、DNS 全地址、MIME、大小、redirect |

## 2.8 练习清理

停止进程、确认锁释放，再删除临时状态。不要提交 `.env`、key、Host credential、真实 URL、数据库、Web
build 或 target。
