# SmartDNS-rs 长期记忆

## GitHub Actions CI 关键经验

### Reusable Workflow 权限规则（重要）
- Caller workflow 的 `permissions` 是被调用 workflow 的**权限天花板**。
- Reusable workflow 只能**降级**权限，**不能提升**。
- 如果 caller 的 permissions 低于 reusable workflow 需求 → `startup_failure`（无错误详情、无日志、无 jobs 运行）。
- **规则**：caller job 的 `permissions` 必须是 reusable workflow 所有 job permissions 的**超集**。
- 调试方法：startup_failure 无日志，只能用二分法排查。

### ghcr.io Docker 推送权限
- 推送 organization/user package 到 ghcr.io 需要 `packages: write` 权限。
- 可在 workflow 级或 job 级声明。
- 匿名验证镜像：先 GET `ghcr.io/token?scope=repository:OWNER/REPO:pull&service=ghcr.io` 取 token，再带 `Authorization: Bearer <token>` 查 `/v2/.../tags/list`。

### PowerShell + GitHub API 陷阱
- `Invoke-WebRequest`/`Invoke-RestMethod` 对 204 No Content 响应抛 "Object reference not set" 异常但请求实际成功。重试循环会创建重复资源。改用 `[System.Net.HttpWebRequest]` 并检查 `StatusCode == 204`。
- 内联 PowerShell `$` 变量在 execute_command 中会被转义吞噬 → 复杂逻辑一律写成临时 `.ps1` 脚本执行。

## 本环境网络/工具备忘
- 代理 `127.0.0.1:7890`（verge-mihomo）对 git TLS 握手不稳定：`git push` 需 `-c http.sslVerify=false` + 多次重试。
- 真实 git：`C:\CommonTools\scoop\apps\git\2.55.0.4\cmd\git.exe`（scoop shim 被不受信任挂载点拦截）。commit 消息用**单引号**。
- 真实 gh：`C:\CommonTools\scoop\apps\gh\2.97.0\bin\gh.exe`。
- cargo 需绕过 rustup shim：直接用工具链 bin + 设 CARGO_HOME/RUSTUP_HOME。勿设 RUSTUP_TOOLCHAIN。
- **本机无 MSVC/Windows SDK**（VS2022 空目录、Windows Kits\10 仅 UnionMetadata，缺 kernel32.lib 等），无法本地 msvc 链接。构建 msvc 二进制须用 GitHub Actions：`gh workflow run build.yml`（workflow_dispatch，不带 version 只出 artifact），用 `gh run download <run> -R owner/repo -n smartdns-<target>-<label>` 下载（label 为 version 或 `main`）。`gh api` 不接受 `-R`，repo 须写进 endpoint。

## SmartDNS 运行优化
- busy 快速失败修复（v0.13.2）：`resource too busy` 从 32% 降至 0%，prefetch median 从 210ms 降至 70ms。
- 局域网发现域名（如 verysync）可加 `address-rule /domain/-` 拦截，避免无效上游查询。
