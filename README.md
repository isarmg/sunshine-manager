# Sunshine Manager

本仓库只包含 Server、管理 Web 和产品协议。Windows/Linux x86_64 客户端及安装流程位于独立的
[sunshine-manager-client](https://github.com/isarmg/sunshine-manager-client) 仓库，固定适配 Sunshine 官方 v2026.516.143833。
客户端通过完整 Git 提交固定本仓库的协议依赖，不需要相邻工作区。拆分前验收仅是历史证据，不代表新仓库版本已完成双平台真实 Sunshine 验收。

管理 Web 支持实例创建与 Client 配对、设备状态、白名单配置预览/编辑、明确授权重启和任务记录。
使用说明见 [实例创建](docs/instance-management.md) 和 [Sunshine 远端管理](docs/remote-management.md)。
主分支的新客户端配对入口和系统证书信任要求见 [简化配对](docs/simple-pairing.md)。

Sunshine Manager `0.10.3` 是独立的 Sunshine 主机管理服务。Server API 采用 sarmg-foundation-server 的
持久管理员控制面；Manager 保存设备身份、任务及审计，不集中保存 Sunshine 管理密码。
实际执行由主机上的独立 Client 完成。Server 使用 Rust/Axum 与 SQLite，内置 Web 使用 Foundation 精确基线的 React/Vite。

项目只接受唯一当前 `/api/v2`、当前 SQLite Schema、凭据 key ID 和不可变发行身份，不注册平行路径，
不读取非当前数据库或其他 key。产品仓不实现迁移、备份和恢复；这些能力归 `sarmg-upgrade` 所有。
升级工具的支持范围以其明确版本矩阵为准，不能将旧 Manager 的备份支持视为当前 Client 状态的支持。
当前 `sunshine:sgev1:` Foundation AES-256-GCM envelope 强制使用确定性、长度分帧的 AAD：
operation request 绑定 operation ID、action 和 `request_ciphertext` 字段域。相同前缀
但使用空 AAD 生成的密文也不是当前格式，启动、doctor 和业务读取都会拒绝，不存在旧密文 fallback。
同一 master key 还通过 HKDF-SHA-256 的两个独立 info 分别派生 request fingerprint 与 Idempotency-Key 的
HMAC-SHA-256 key；SQLite 中没有低熵请求或幂等键的裸 SHA-256 摘要，也不接受旧摘要兼容。

浏览器源码统一位于 `web/`；运行配置模板位于 `config/`。业务 DDL 位于 `schema/product.sql`，
完整当前 DDL 由 Foundation Schema Composer 写入 `schema/generated/current_schema.sql`。
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
cd web && npm ci && npm run build
```

## 文档

- [文档总览](docs/README.md)
- [初学者学习指南](docs/beginner-guide/README.md)
- [项目工作流程与流程树](docs/project-workflow.md)
- [完整功能与取舍清单](docs/feature-inventory-and-tradeoffs.md)
- [部署、配置、安全与故障运维](docs/operations.md)

代码采用 [Apache License 2.0](LICENSE-APACHE)。

账号修改方法见 [账号设置](docs/account-settings.md)。
