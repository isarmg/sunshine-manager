# 08. 测试、调试与变更方法

## 8.1 基础质量门

```bash
cargo +1.98.0 fmt --all -- --check
cargo +1.98.0 check --locked --target x86_64-unknown-linux-gnu --all-targets
cargo +1.98.0 clippy --locked --target x86_64-unknown-linux-gnu --all-targets -- -D warnings
cargo +1.98.0 test --locked --target x86_64-unknown-linux-gnu
cd clients/web
npm ci
npm run build
```

还需运行 workflow policy、release package 静态/自测和 `git diff --check`。发行/Schema/crypto 修改必须跑
真实重定位和篡改负例。

## 8.2 调试层次

release/config -> database/key/lock -> Session/CSRF -> operation persistence -> worker queue -> Sunshine
network/API -> terminal/outbox -> Web。operation ID 是产品内持久关联键；Foundation error 中的 request ID
虽为可选字段，但当前 Server 不生成，不能把它作为排障前提。

## 8.3 认证测试

测试未知账户等成本、Argon2 并发、body 限制、Session idle/absolute TTL、登出、CSRF、Origin/Host、
forwarded header 被忽略/transport peer 作为来源和 Secret redaction。

## 8.4 Operation 测试

测试幂等、两个 HKDF/HMAC 域的稳定/隔离/换 key/非裸 SHA、Host 后写覆盖的当前语义、per-Host 串行、不同 Host 公平、所有网络断点、重启 unknown 和审计 outbox。不要只
mock “200 OK”。

## 8.5 封面测试

使用受控 DNS/HTTP fixture，覆盖地址分类、解析变化、pin、redirect、size、MIME、一次性授权、peer 绑定
和超时。测试不得访问真实 metadata/内网地址。

## 8.6 数据库与发行

覆盖当前新库、空文件、metadata 假报、DDL drift、错误 key、损坏 envelope、双实例、maintenance 锁、
manifest extra/missing/tamper、symlink/hardlink/mode 和重定位。

## 8.7 变更联动

API 同步 Rust/Web/测试；Schema/crypto 同步 identity/doctor，并仅在升级仓新增经评审的具体 adapter/edge；
release 同步打包器/运行时/部署；名称同步 package/binary/config/docs。替换当前合同时删除被替代入口，
不做 dual read/write。

## 8.8 提交前

全文与路径搜索非当前产品身份为零；文档链接/命令有效；没有 Secret、数据库、dist/node_modules/target；完整门禁
通过；每个大问题单独提交便于审计和回滚。
