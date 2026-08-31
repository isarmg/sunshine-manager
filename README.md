# Sunshine Manager

Sunshine Manager `0.7.0` 是独立的 Sunshine 主机管理服务。Server API 提供基于 canonical username 的
本地管理员身份、主机凭据管理、应用/客户端控制和可恢复的异步远程操作；当前内置 Web 提供登录、会话
恢复与 Host 只读概览。Server 使用 Rust/Axum 与 SQLite，内置 Web 使用 Foundation 精确基线的 React/Vite。

项目只接受唯一当前 `/api/v2`、`0.7.0` SQLite Schema、凭据 key ID 和不可变发行身份，不注册平行路径，
不读取非当前数据库或其他 key。产品仓不实现迁移、备份和恢复；这些能力归 `sarmg-upgrade` 所有，但只有
该仓库将来明确登记并验证具体 Sunshine 转换边后才属于支持范围，目前没有可执行的 Sunshine 历史边。
当前 `sunshine:v1:` AES-256-GCM envelope 强制使用确定性、长度分帧的 AAD：Host credential 绑定 Host ID
和 `secret` 字段域，operation request 绑定 operation ID、action 和 `request_ciphertext` 字段域。相同前缀
但使用空 AAD 生成的密文也不是当前格式，启动、doctor 和业务读取都会拒绝，不存在旧密文 fallback。
同一 master key 还通过 HKDF-SHA-256 的两个独立 info 分别派生 request fingerprint 与 Idempotency-Key 的
HMAC-SHA-256 key；SQLite 中没有低熵请求或幂等键的裸 SHA-256 摘要，也不接受旧摘要兼容。

浏览器源码统一位于 `clients/web/`；运行配置模板位于 `config/`，当前数据库 DDL 位于根目录 `schema.sql`。
真实数据库、credentials key
和生产环境文件位于源码树外。

正式 Server binary 及其内置 Web 发行树只支持 `x86_64-unknown-linux-gnu`（Linux AMD64）。这是控制面
发行边界，不改变被管理 Sunshine Host、Moonlight 客户端或 Sunshine 上游协议的原有平台范围。

## 快速验证

```bash
python3 scripts/check-workflow-supply-chain.py
cargo +1.98.0 fmt --all -- --check
cargo +1.98.0 check --locked --target x86_64-unknown-linux-gnu --all-targets
cargo +1.98.0 clippy --locked --target x86_64-unknown-linux-gnu --all-targets -- -D warnings
cargo +1.98.0 test --locked --target x86_64-unknown-linux-gnu
cd clients/web && npm ci && npm run build
```

## 文档

- [文档总览](docs/README.md)
- [初学者学习指南](docs/beginner-guide/README.md)
- [项目工作流程与流程树](docs/project-workflow.md)
- [完整功能与取舍清单](docs/feature-inventory-and-tradeoffs.md)
- [部署、配置、安全与故障运维](docs/operations.md)

代码采用 [Apache License 2.0](LICENSE-APACHE)。
