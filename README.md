# Sunshine Manager

Sunshine Manager `0.7.0` 是独立的 Sunshine 主机管理服务，提供本地管理员身份、Web 控制台、主机凭据
管理、应用配置和可恢复的异步远程操作。服务端使用 Rust/Axum 与 SQLite，Web 使用 React/Vite。

项目只接受当前 `/api/v2`、`0.7.0` SQLite Schema、凭据 key ID 和不可变发行身份，不注册旧路径、
不读取旧数据库或 previous key，也不提供迁移、备份和恢复；这些工作由 `sarmg-upgrade` 离线完成。

浏览器源码统一位于 `clients/web/`；仓库内受审 Schema 合同位于 `config/`。真实数据库、credentials key
和生产环境文件位于源码树外。

## 快速验证

```bash
python3 scripts/check-workflow-supply-chain.py
cargo +1.98.0 fmt --all -- --check
cargo +1.98.0 check --locked --all-targets
cargo +1.98.0 clippy --locked --all-targets -- -D warnings
cargo +1.98.0 test --locked --all-targets
cd clients/web && npm ci && npm run build
```

## 文档

- [文档总览](docs/README.md)
- [初学者学习指南](docs/beginner-guide/README.md)
- [项目工作流程与流程树](docs/project-workflow.md)
- [完整功能与取舍清单](docs/feature-inventory-and-tradeoffs.md)
- [部署、配置、安全、备份与故障运维](docs/operations.md)

代码采用 [Apache License 2.0](LICENSE-APACHE)。
