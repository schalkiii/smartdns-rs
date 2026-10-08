# SmartDNS-rs 全面代码 Review 与改进计划

> 生成于 2026-07-26 · 基于 `src/dns_mw_cache.rs`、`src/dns_client.rs`、`src/api/stats.rs`、`src/api/cache.rs`、`src/dns_mw_addr.rs` 与部署配置 `smartdns.conf` 的实测与通读。
> 本轮在 2026-07-08 PERFORMANCE_REVIEW 的基础上继续深入，重点核查既有优化是否真正落地，并发现新的改进点。

## 0. 既有优化核查（已确认真正落地，非纸面）

| 优化 | 验证位置 | 结论 |
|------|----------|------|
| 上游熔断（连续失败计数 + 冷却跳过） | `dns_client.rs:522-623`、`651-654` | ✅ 真实存在且正确接入 `NameServerGroup` fan-out |
| 缓存分片锁 | `dns_mw_cache.rs` `shards: Vec<Arc<Mutex<LruCache>>>`、`shard_index_of` | ✅ 16 分片，按查询路由 |
| 预取就绪堆（非全表扫描） | `get_expired` 仅 `peek/pop` 堆顶 | ✅ 已用 `prefetch_heap` |
| 否定缓存（NXDOMAIN/NODATA，含 CNAME+NODATA） | `insert_negative` / `is_negative_response(resp, query)` | ✅ 命中计数正确 |
| `DomainPrefetchingNotify` 定时器泄漏修复 | 改为 `notify_one()` + `sleep_until` 单定时器 | ✅ 已部署（7/24 build） |
| 配置清理（去 `force-aaaa-soa` / `dualstack-ip-selection`） | `smartdns.conf` 注释与正文 | ✅ 已落地 |

## 1. 本轮已实施的改进

### 1.1 [P1·性能] `/stats` 每次轮询全量克隆整个缓存 → O(1)
- **问题**：`src/api/stats.rs:37` 每次 `/stats`（WebUI 高频轮询）都调用 `cached_records()`，**克隆最多 `cache-size`(65536) 条记录**，只为算出有歧义的 `cache_hits` 字段（各条目命中计数之和，跨重启累加、与 `total_queries` 不可比）。这是 dashboard 刷新时的隐藏全表扫描。
- **修复**：
  - `cache_hits` 与 `cache_query_hits` 统一改用 O(1) 的全局命中原子计数 `query_hits`（`DnsCache::query_hits()`）；
  - `cache_size` 改用 `entry_count()`（仅跨 16 个分片各取 `.len()`，O(分片数) 而非 O(条目数)）。
  - `cached_records()` 仍保留给缓存管理列表接口（`api/cache.rs:35`），不受影响。
- **收益**：dashboard 轮询从「克隆数万条记录」降为「几次原子读 + 16 次轻量锁」，长缓存下延迟尖刺消除。

### 1.2 [P2·性能] 命中即预取（on-hit prefetch）去冗余
- **问题**：`handle()` 在**每一次**缓存命中（Valid / Expired 分支）都调用 `try_prefetch` 向后台入队一次上游重查。堆驱动预取（`get_expired`）本已在到期时刷新，命中即预取只是补充，但「每个命中都打上游」对长 TTL 热域名是持续无谓的查询量（受 channel 容量与信号量限流，但仍有常驻后台流量）。
- **修复**：新增门槛常量 `ON_HIT_PREFETCH_REMAINING_TTL = 600`(秒)。仅当缓存条目**剩余 TTL < 600s** 时才在命中时触发后台刷新；serve-stale 条目 TTL 已被改写为回复 TTL（远低于门槛），照常刷新。临近过期即刷新的保鲜效果不变，长 TTL 条目的无谓后台流量被消除。
- **收益**：热域名后台预取查询量显著下降，上游压力与日志量同步降低。

## 2. 已确认「不是问题」的点（避免误改）

- **`NameServerGroup::lookup` 只在第一条非空 `Ok` 时返回**：看似会拖慢否定应答，但这是**有意设计**——避免某个抖动上游先返回 `NXDOMAIN` 导致误否定。保持不变。
- **锁顺序**：`insert` 先释放分片锁再获取堆锁；`get_expired` 持堆锁再取分片锁。二者不存在同时持两锁的嵌套，**无死锁**。
- **`serve-expired` + 否定缓存**：否定条目不进 prefetch 堆，但命中时原样返回并计为命中，逻辑自洽。

## 3. 子代理深度审查发现（P1–P3）

并行 code-explorer 子代理（Explore-1）对 `src/app.rs`、`src/dns_client.rs`、`src/dns_mw_ns.rs` 等做了更深审查，返回一批 `unwrap`/panic/逻辑缺陷发现。本轮已逐条核查并处置，结果如下（原始报告称 13 项，其中 1 项与本轮 §1.1 的 `entry_count()` 优化重叠，故此处列出 12 条独立项）：

| # | 位置 | 严重度 | 问题 | 处置 |
|---|------|--------|------|------|
| 1 | `app.rs:381-417` | P1·正确 | 背景（预取）请求在空闲期或 `bg_batch` 满时，既不入 `bg_batch` 也不入 `batch`，被静默丢弃；`sender` 永不回送，等待方 task 与 `active_queries` 计数双重泄漏 | ✅ 已修复：空闲/满时降级进 `batch` 即时处理并回送响应 |
| 2 | `app.rs:584-590` | P1·崩溃 | `OpCode::Status/Notify/Update/Unknown` 与 `MessageType::Response` 走 `todo!()`，命中即 `panic!`，task 崩溃、查询永久挂起 | ✅ 已修复：返回 `NotImp` / `Refused` 空响应 |
| 3 | `dns_client.rs:715-742` | P1·正确 | `Ok(response)` 一律 `record_success()`，但 SERVFAIL/REFUSED 是合法响应，导致持续出错的上游永不触发熔断冷却 | ✅ 已修复：对 `ServFail`/`Refused` 改调 `record_failure()` |
| 4 | `dns_client.rs:486` | P1·资源 | `NameServerGroup::lookup` 用 `tokio::spawn` 派发各上游查询，调用方 drop 后 task 不被取消 | ⚠️ 有意保留：取消 task 会重演旧方案「僵尸请求」（DnsMultiplexer 槽位长期占用）。当前 task 受单上游 `NS_QUERY_TIMEOUT` 约束会自然结束，属可接受权衡；已在代码注释说明 |
| 5 | `dns_client.rs:603,617` | P2·健壮 | `last_failure.lock().unwrap()` 在 Mutex 中毒时 panic | ✅ 已修复：`unwrap_or_else(\|e\| e.into_inner())` |
| 6 | `dns_client.rs:87-105` | P2·健壮 | `is_retryable` 用 `err.to_string().contains("receiver was canceled")` 与 OS 码嗅探，hickory 改文案即失效 | ✅ 已修复：直接匹配 `ProtoErrorKind::Busy/Canceled/Io(资源耗尽码)` |
| 7 | `dns_mw_ns.rs:229` | P2·崩溃 | `IpStrategyResult::Fallback` 的 ok/err 均为空时 `unreachable!()` panic | ✅ 已修复：返回 `Err(NoConnections)` |
| 8 | `app.rs:597` | P2·健壮 | FormError 分支 `kind.into_form_error().expect(...)`，守卫已确认存在但属脆弱写法 | ✅ 已修复：`if let Some` 分支，否则返回空响应 |
| 9 | `dns_client.rs:57` | P3·文档 | 注释写「并发数设为 16」但常量实为 `12` | ✅ 已修复：注释对齐为 12 |
| 10 | `app.rs:161` | P3·清晰 | `add_stats_snapshot(cache_hits)` 形参名与调用处传入的 `cache_query_hits` 语义不符 | ✅ 已修复：形参改名为 `query_hits` |
| 11 | `dns_mw_cache.rs:647` | P3·清理 | `entry_count()` 报告称「无生产调用方」 | ↺ 已反驳：本轮 §1.1 的 `/stats` 已改用 `entry_count()`（`api/stats.rs:41`），存在生产调用方，无需删除 |
| 12 | `dns_client.rs:171` | P3·可观测 | 上游引用的代理名在 `proxies` 中找不到时静默 `None`，配置错误难发现 | ✅ 已修复：找不到时 `warn!` 提示并忽略 |

> 说明：#4 经核查属有意设计（注释已说明僵尸请求权衡），不改动；#11 与前轮优化重叠，已落实而非删除。其余 10 项均已落地，随本轮构建（见 §5）部署。

### 3.2 第二轮深度审查（服务器/代理/审计/配置，P1–P3，2026-07-26 续）

并行 code-explorer 子代理（Explore，agent-c8c15cfe）通读了 `src/server/*`、`src/proxy.rs`、`src/dns_mw_addr.rs`、`src/dns_mw_audit.rs`、`src/dns_conf.rs`、`src/resolver.rs`、`src/server/https.rs` 等此前未覆盖的模块，返回 16 项发现（原报 14，实际为 16）。本轮逐条核查并处置：

| # | 位置 | 严重度 | 问题 | 处置 |
|---|------|--------|------|------|
| R1 | `proxy.rs:103` | **P1·崩溃** | `connect_udp` 的 `ProxyProtocol::Http` 分支直接 `unimplemented!()`；UDP 上游误配 HTTP 代理即在查询 task 内 panic | ✅ 已修复：返回 `io::Error(Unsupported)` 由 dns_client 正常降级 |
| R2 | `server/net.rs:29` | P2·崩溃 | 单个监听器 `bind` 失败即 `panic!("cound not bind...")`（拼错且整进程退出），哪怕其他地址正常 | ✅ 已修复：`bind_to` 返回 `io::Result<T>`，7 处调用点加 `?`，错误以 `crate::Error` 干净上抛 |
| R3 | `server/tls.rs:77` | P2·DoS | DoT 的 `tls_acceptor.accept()` 无超时：客户端建连后不完成握手即永久挂起 task + socket（slowloris） | ✅ 已修复：外层包 `tokio::time::timeout(timeout, ...)`，握手超时直接关闭 |
| R4 | `dns_mw_audit.rs:50` | P2·尾延迟 | 审计 `audit_sender.send().await` 在请求返回路径上阻塞（有界通道 cap=100）；写盘慢时所有 DNS 响应被审计速度拖住 | ✅ 已修复：`try_send` 满则丢弃（审计允许丢失），不再阻塞请求路径 |
| R5 | `dns_conf.rs:1037` | P2·崩溃 | `conf-file` 指向无读权限文件时 `load_file().expect("load_file failed")` 启动即 panic | ✅ 已修复：`if let Err` → `error!` 继续加载 |
| **R6** | `dns_mw_cache.rs:get_expired` | **P1·死锁** | `get_expired` 持 `prefetch_heap` 锁并在循环内 `await` 各分片锁，而 `insert` 是「分片→堆」顺序——**锁顺序反转，并发下死锁**（预取 worker 与缓存写入相互等待、双向挂起，触发查询永久挂起） | ✅ **已修复**：每次只从堆弹出一个候选、立即释放堆锁，再单独锁分片判定；需保留的条目统一放回堆。同时 `clear()` 补清 `prefetch_heap` |
| R7 | `dns_mw_cache.rs:744` | P3·一致 | `clear()` 不清空 `prefetch_heap`，重载/rcache 后堆残留旧 Query | ✅ 已修复：`clear()` 同步 `prefetch_heap.lock().clear()` |
| R8 | `dns_conf.rs:117/125` | P3·启动 | 找不到配置文件/`load` 失败用 `panic!` | ↺ 文档化建议：启动期 panic 可改为 `Err` 由 `main` 统一退出；本轮未改（仅启动阶段、影响可控） |
| R9 | `dns_conf.rs:1107` | P3·可观测 | 拼写错误指令（如 `cache-szie`）仅 `warn!`，默认日志级下不可见，运维误以为生效 | ✅ 已修复：升级为 `error!` |
| R10 | `dns_mw_audit.rs:189` | P3·健壮 | 审计写线程 `writer.into_inner().expect("read csv peamble")` 在后台 task 内 panic → 审计静默停止 | ✅ 已修复：`if let Ok(buf)` 失败则 `warn!` |
| R11 | `server/mod.rs:247` / `udp.rs:58` | P3·资源 | UDP 每包 `spawn` 一个 task + 无界通道，查询洪水下 task 数与消息数无界增长（内存放大） | ↺ 文档化建议：对并发查询加 `Semaphore` 限流 / 无界通道改有界；本轮未改（结构性改动，需压测验证） |
| R12 | `server/udp.rs:61` / `tcp.rs:82` | P3·健壮 | 服务器侧 `handler.send()` 无端到端总超时，解析链异常挂起时 UDP/TCP task 长期存在 | ↺ 文档化建议：外包 `tokio::time::timeout(整体预算)` 超时返回 SERVFAIL；本轮未改（需确定合理预算，避免误杀慢查询） |
| R13 | `resolver.rs:161` | P3·健壮 | CLI `arg.into_string().expect("Failed to convert OsString to String")` 非 UTF-8 参数直接 panic | ✅ 已修复：返回 `Err`，`try_parse_from` 上层报错退出 |
| R14 | `server/https.rs:48` | P3·健壮 | `alt_svc` 的 `HeaderValue::from_str().expect()`（作者留 TODO） | ✅ 已修复：`unwrap_or_else(\|_| HeaderValue::from_static(""))` |
| R15 | `dns_mw_addr.rs:189` | P3·防御 | `match query_type { A=>, AAAA=>, _=>unreachable!() }` 若未来扩展 IP 类记录类型会变真实 panic | ✅ 已修复：`_ => return None` |
| R16 | `dns_mw_cache.rs:647/750` | P3·性能 | `entry_count()`/`cached_records()` 全分片扫描 | ↺ 评估为可接受：`entry_count` 仅 O(分片数)=O(16)；`cached_records` 是缓存列表 API（非每条查询热路径），大缓存下才显著，按需分页即可 |

> **本轮最高优先级是 R6（死锁）**：原 §2「锁顺序·无死锁」的结论是**错的**——`get_expired` 确实在持有堆锁期间 `await` 分片锁，与 `insert` 的分片→堆顺序构成反转。已通过「释放堆锁后再锁分片」彻底消除。R1/R2/R3/R4/R5 均为可达路径上的 panic / 阻塞 / DoS，已修复。

## 4. 建议（待定，需权衡/用户授权）

| 优先级 | 项 | 说明 | 处置 |
|--------|----|------|------|
| P3·配置 | `serve-expired-ttl 604800`（7 天）过激 | 上游宕机超过 7 天会持续返回陈旧 IP。建议降到 `86400`(1 天) 或更小，平衡「抗抖动」与「新鲜度」 | 建议改配置并重启；行为变更，需用户确认 |
| P3·功能 | `speed-check-mode none` 下 AAAA 永无真实 IPv6 | 当前上游对 AAAA 直连返回 CNAME 较快，故可用；若日后 IPv6 链路改善，应恢复测速而非长期 `none` | 文档记录，非代码改动 |
| P4·可观测 | `audit-enable yes` 在 `log-level debug` 下 audit.log 增长快 | 审计与调试日志分文件，互不干扰；如嫌吵可关 audit 或降 log-level | 按需 |
| — | 上游当前仅 UDP + DoT（所有 DoH/QUIC 已注释） | 直连网络下 DoH 被墙，现状合理；若配 `proxy-server` 走本地代理 127.0.0.1:7890 可重新启用 DoH | 文档记录 |

> 注：`src/dns_client.rs` / `src/dns_mw_dualstack.rs` 等子系统的更深审查由并行 code-explorer 子代理进行中，其发现的 `unwrap`/配置校验等补充项将在此文件追加。

## 5. 部署
- 构建：`cargo build --release --features web-ui` ✅ 已完成（2026-07-26，exe 位于 `target/release/smartdns.exe`）。
- 本轮关键修复：**R6 死锁**（预取 worker 与缓存写入锁反转）**强烈建议尽快部署**——长期运行高并发下可能触发查询挂起。
- 步骤（沿用项目约定）：停掉运行实例（PID 13968，session 0）→ 备份 `smartdns.exe` → 覆盖 `C:\CommonTools\smartdns-x86_64-pc-windows-msvc\smartdns.exe` → 以原方式重启。
- 注意：当前实例在 session 0 运行且**未**注册为 SCM 服务，停止可能需要提权；若 agent 无权停止，将交付构建好的 exe 并给出用户侧重启命令。
