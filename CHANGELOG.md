# Changelog

本文件记录 smartdns-rs 的变更历史。格式遵循 [Conventional Commits](https://www.conventionalcommits.org/)。

## [Unreleased]

### fix(dns_mw_cache): 恢复否定缓存（negative caching）以提升命中率

**问题**：Web UI 显示缓存命中率长期仅 44%，而原版 smartdns 可达 90%+。

**根因**：`dns_mw_cache.rs` 中 `negative_ttl` 相关逻辑被注释掉，导致 NXDOMAIN / NODATA（否定应答）完全不被缓存。实测最近 1 小时真实客户端查询中 **47.1% 为否定应答**（绝大多数是双栈域名的 AAAA 查询无 AAAA 记录），这些每次都被重新向上游解析，全部计为未命中。

**验证**（直连 127.0.0.1:9053 受控实测）：
- 正向缓存（5 个真实域名各查 2 次）→ 第二次 100% 命中 ✅
- serve-expired（24 个已过期条目）→ 83% 命中 ✅（说明 600s TTL 硬上限被 7 天陈旧窗口掩盖，非命中率主因）
- 否定缓存（`rdelivery.qq.com` AAAA，已知 NXDOMAIN，查 2 次）→ miss +2、hit +0 ❌

**修复**：
- `dns_mw_cache.rs`：对 NXDOMAIN / NODATA 响应调用新增 `insert_negative()`，按 SOA minimum TTL（限幅 60s~3600s）入缓存；命中时原样返回并计为 hit。SERVFAIL / REFUSED 仍不缓存；否定条目不进 prefetch 堆。
- `dns_conf.rs` / `config/cache.rs`：新增 `cache-negative` 配置（默认 `yes`，对齐原版）。
- `smartdns.conf`：已显式加 `cache-negative yes`。

**说明**：命中率公式 `cache_query_hits / total_queries` 本身正确（`hits + misses == total` 自洽）；界面 `cache_hits` 字段为各条目命中计数之和（跨重启累加），与 `total_queries` 不可比，仅为展示用，不影响真实命中率。

### fix(dns_mw_addr): 修复 AddressMiddleware 重建响应丢失 from_cache 标记

**问题**：Web UI 仪表盘显示矛盾的缓存统计——命中率显示 76.6%（4414 命中 / 5766 总查询），但下方却显示"0 命中 + 5766 未命中"、"缓存命中耗时 0.0ms（0 次命中）"、"上游查询耗时 1290.5ms（5766 次未命中）"。

**根因**：`AddressMiddleware` 在 `next.run()` 返回后，当 records 被修改（`Cow::Owned` 路径）时用 `DnsResponse::new_with_deadline` 重建响应。该构造函数重置 `from_cache: false` 和 `name_server_group: None`，丢失缓存命中标记。用户配置了 `max-reply-ip-num`、`rr-ttl-min`、`rr-ttl-max`、`rr-ttl-reply-max` 都会触发 `Cow::Owned` 路径，导致几乎所有缓存命中的响应都被误算作未命中。

**数据流**：缓存命中时 `dns_mw_cache.rs` 调用 `res.mark_from_cache()` 和 `query_hits +1`（两个计数器都正确），但响应返回经过 `AddressMiddleware` 时被重建，`from_cache` 丢失。`app.rs` 的 `lookup.is_from_cache()` 返回 false，`cache_hit_queries` 始终为 0。前端命中率用 `cache_query_hits`（正确），但命中/未命中数用 `cache_hit_queries`（错误，恒 0）和 `cache_miss_queries`（错误，等于总查询数）。

**修复**：在 `Cow::Owned` 重建后保留原 lookup 的 `from_cache` 和 `name_server_group` 标记。添加两个回归测试验证 TTL 调整和 `max-reply-ip-num` 截断场景下 `from_cache` 标记的保留。

### style: 修复 CI cleanliness 检查失败

`cargo fmt --check` 在 5 个文件检测到格式 diff（行宽换行），导致 GitHub Actions 的 cleanliness job 全平台失败。运行 `cargo fmt --all` 统一格式后通过。涉及文件：app.rs、dns_client.rs、dns_mw_cache.rs、dns_mw_ns.rs、connection_provider_tests.rs。

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
