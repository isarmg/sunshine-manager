# Sunshine Manager 运维文档

## 1. 生产布局

```text
/opt/isarmg/sunshine-manager/releases/0.7.0/  root-owned read-only release
/etc/isarmg/sunshine-manager.env             0600 environment
/var/lib/isarmg/sunshine-manager/db/app.db   SQLite
/run/isarmg/sunshine-manager/                locks/runtime
```

systemd 使用 `isarmg-sunshine`，直接执行：

```text
/opt/isarmg/sunshine-manager/releases/0.7.0/bin/sunshine-manager \
  serve-release --root /opt/isarmg/sunshine-manager/releases/0.7.0
```

发行树无 `current` 链接，不依赖工作目录，必须拒绝 symlink、特殊文件、hardlink asset、服务账户拥有的
asset 和 group/world writable 内容。

## 2. 构建与安装发行物

从干净且 annotated `v0.7.0` 精确指向 HEAD 的 checkout：

```bash
python3 scripts/package-release.py /absolute/release-output
```

输出已存在时拒绝覆盖。安装：

```bash
tar -xzf sunshine-manager-0.7.0-x86_64-unknown-linux-gnu.tar.gz \
  -C /opt/isarmg/sunshine-manager/releases
/opt/isarmg/sunshine-manager/releases/0.7.0/bin/sunshine-manager \
  verify-release --root /opt/isarmg/sunshine-manager/releases/0.7.0
```

安装仓库内 systemd unit，创建专用账户、状态目录和 `/etc/isarmg/sunshine-manager.env`，再 enable/start。
同版本不得合并或覆盖。

## 3. 环境配置

| 变量 | 默认/要求 | 说明 |
|---|---|---|
| `SUNSHINE_MANAGER_DATABASE_URL` | 必填 | 当前 SQLite URL |
| `..._BIND` | `127.0.0.1:18104` | 生产回环监听 |
| `..._STATIC_DIR` | 必填固定 release Web | 不能是 symlink |
| `..._CREDENTIAL_KEY_ID` | 当前 key ID | 与备份 manifest 一致 |
| `..._CREDENTIAL_KEY` | Base64 32 字节 | 独立秘密管理，禁止日志/仓库 |
| `..._BOOTSTRAP_ADMIN_*` | 首次库初始化 | 使用后轮换初始密码 |
| `..._SESSION_TTL_SECONDS` | 43200 | 绝对期限 |
| `..._SESSION_IDLE_TTL_SECONDS` | 1800 | 空闲期限 |
| `..._SESSION_COOKIE_SECURE` | 生产 true | 只通过 HTTPS |
| `..._COVER_URL_ALLOWLIST` | 默认空 | 逗号分隔精确 DNS host |
| `..._COVER_PROXY_ORIGIN` | allowlist 非空时必填 | Sunshine 主机直达 HTTPS origin |

## 4. 管理命令

```bash
sunshine-manager identity
sunshine-manager verify-release --root /opt/isarmg/sunshine-manager/releases/0.7.0
sunshine-manager doctor
sunshine-manager admin-create --database-url sqlite:///path/app.db
sunshine-manager admin-reset-password --database-url sqlite:///path/app.db \
  --email admin@example.com --password '<new-secret>'
```

管理员写命令需要 maintenance exclusive；先停服务。避免把真实密码留在 Shell history，使用受控 Secret
注入或临时受保护终端。

## 5. Doctor

`doctor` 验证 product metadata、现场 Schema fingerprint、SQLite integrity/foreign keys、可回滚写事务，
并用当前 key 解密全部 Host credential 和未完成 operation request。它不连接 Sunshine，也不保留写探针。

若 Schema 不符，不得手改 metadata；若解密失败，先确认 key ID、key 文件来源和权限，不得添加旧 key
fallback。

## 6. 反向代理和网络

公网 TLS proxy 保留原 Host，并确保 Session Cookie Secure。应用端口只对 proxy 回环开放。Sunshine
Host 地址由防火墙限制，优先使用 HTTPS 且验证证书。

封面代理 origin 必须由 Sunshine Host 直接访问，不能把一次性路径经过公共 proxy；两端网络都要拒绝
private/link-local/loopback/metadata egress 和危险 redirect。

## 7. 备份、恢复、升级和换 key

产品没有相关写入实现。使用 `sarmg-upgrade`，一致性单元至少包含 SQLite generation 与 external
credential key 身份。Sunshine 当前备份、验证和恢复都要求 `--credentials-key-id` 与受保护 key file，
工具会认证所有密文后才发布或安装备份；原始 key bytes 不进入备份。

在线一致性备份可取得 maintenance shared；恢复、版本升级和 re-encryption 必须停服务并取得 exclusive。
定期在隔离环境演练，尤其验证 pending/unknown operation 和 Host 凭据。

## 8. 监控与故障定位

1. 检查 systemd、release verify 和 Web asset 是否通过。
2. 检查反向代理 TLS、Cookie、Origin/Host 与系统时钟。
3. 运行 doctor，确认 SQLite、写能力和全部密文。
4. 对 pending 查看 worker/Host 网络；对 unknown 先查 Sunshine 实际状态，禁止盲重试。
5. 对封面错误检查 allowlist、DNS 的全部地址、MIME/大小、proxy origin 和 Host egress。
6. 监控 SQLite/WAL、operation backlog、unknown 数量、磁盘和 inode。

## 9. 安全事件

隔离公网和受影响 Sunshine Host，保全 release SHA、数据库 generation、审计和日志，轮换管理员 Session/
密码、credential key、Sunshine 凭据、TLS key，并通过外部工具完成 re-encryption。使用 GitHub Private
Vulnerability Reporting；公开 issue 不得包含生产 Host、数据库、密文、key、URL 或请求正文。只支持
当前发布版本与当前 `main`。
