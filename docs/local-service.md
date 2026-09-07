# 本地启动 Sunshine Manager

本地源码服务（不是 systemd 生产安装）只监听 `127.0.0.1:18104`。不安装或启动受管 Sunshine Host，也不自动注册 Host。

```sh
npm ci --prefix web
npm run build --prefix web
CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_BUILD_JOBS=2 cargo build --locked
node scripts/local-service.mjs start
node scripts/local-service.mjs status
node scripts/local-service.mjs stop
```

浏览器访问 `http://127.0.0.1:18104`。管理员为 `admin`；首次启动随机生成密码，保存于权限为 0600 的 `.runtime/local-service/login.txt`，不写日志或 Git。独立数据库及密钥保留在忽略目录 `.runtime/local-service/`，重启使用同一份状态。不要删除或替换现有密钥文件。

只在回环监听下使用产品已有的开发 HTTP Session 模式。停止命令验证记录的进程启动时间和二进制路径，只向匹配的本地服务发送 SIGTERM；不强制杀死其他占用端口的进程。

当前管理 Web 的默认字体来自 Server Foundation 维护的 `web/fonts/` 摘要快照：英文为 Maple Mono Normal NL 正体（非斜体、非手写、无连字），中日文为 Maple Mono NL CN 正体。字体分片按需由本地服务提供，不依赖访问者安装字体或外部 CDN。
