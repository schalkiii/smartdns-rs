# Changelog

本文件记录 smartdns-rs 的变更历史。格式遵循 [Conventional Commits](https://www.conventionalcommits.org/)。

## [Unreleased]

### fix(dns_client): 消除 NameServerGroup 僵尸请求根因

**问题**：长时间运行后出现大量 `resource too busy` 错误（实测 1057 次/h），并伴随网络查询异常。

**根因**：`NameServerGroup` 使用 `FuturesUnordered` + 早期 return，首个有效结果返回时 drop 其余 future。被 cancel 的请求在 `DnsMultiplexer` 的 `active_requests` 中遗留 request ID，形成"僵尸请求"。慢速上游服务器（5s 超时）让僵尸长期占用 32 个槽位，导致新请求触发 `Busy` 错误。

**修复**：改用 detached task + mpsc channel，每个服务器查询独立 spawn。首个有效结果返回后，其余 task 在后台自然完成并释放槽位，不产生僵尸。`PER_NAMESERVER_CONCURRENCY` 恢复为 16（实测数据表明降至 8 反而恶化 busy 率 3 倍）。

### merge: 同步上游 upstream/main (v0.13.1)

同步上游 7 个提交：

- **feat: PTR 反向查询支持 LAN 主机** — 解析 PTR 查询名称提取 IP 地址，从 dhcp.leases 文件查找主机名，返回 PTR 记录。支持 IPv4/IPv6。
- **feat: 中间件重排序** — `DnsmasqMiddleware` 提前到 `DnsZoneMiddleware`/`DnsHostsMiddleware` 之前，使 dnsmasq 租约查找优先于 hosts 文件。
- **tweak: 依赖更新 + let-chain 重构** — 大规模依赖版本更新，移除 patch-crate，代码统一使用 `if let X && let Y` 语法。
- **docs: AGENTS.md 代码原则、贡献指南、git 提交策略**
- **chore: 版本升至 0.13.1**

### feat(stats): 区分缓存命中/未命中/预取的查询时间统计

Web UI 实时显示四项指标：综合平均查询时间、缓存命中耗时、上游查询耗时、后台预取耗时。

### fix(dns_client): per-NameServer 信号量替代全局信号量

每个服务器独立限制并发请求数，避免 `NameServerGroup` cancel 产生的僵尸请求连锁影响其他服务器。

### fix: DNS 查询并发限制与重试逻辑

- 添加全局 DNS 查询并发限制器
- 为 `resource too busy` 错误添加重试逻辑（指数退避 50ms→1s，最多 5 次）
- 扩展重试覆盖 `receiver was canceled` 错误
- 修复重试死循环

### perf: DNS 中间件管线冗余分配优化

### refactor: 拆分 lookup_ip 策略

将 `lookup_ip` 策略拆分为独立函数（`FirstPing`、`FastestIp`、`FastestResponse`），用 `FuturesUnordered` 替代 `select_all`，缓存 `server_group_name`。

## [0.13.0] - 2026-06

### feat: Web UI 仪表盘

内置 Web 仪表盘，支持系统概览、上游服务器检查、缓存管理（搜索/刷新）和规则管理。

### feat: 高级统计与监控

分别跟踪前台和后台查询指标，包括查询次数和平均响应时间。

### feat: 缓存预取优化

智能缓存预取，可配置批量限制、最小间隔和指数退避。
