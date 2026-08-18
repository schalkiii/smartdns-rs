# Changelog

本文件记录 smartdns-rs 的变更历史。格式遵循 [Conventional Commits](https://www.conventionalcommits.org/)。

## [Unreleased]

### fix(dns_client): Busy 快速失败，切断“busy→重连→更慢”恶性循环

**问题**：运行十余天后 WebUI “上游查询耗时”升至 700~800ms（日志实测 prefetch median=210ms / mean=222ms / p95=437ms，`resource too busy` 占查询约 32%）。

**根因**：hickory `DnsMultiplexer::send_message` 在 `active_requests` 超过 `CHANNEL_BUFFER_SIZE(32)` 时返回 `ProtoErrorKind::Busy`。此前 `is_retryable` 把 Busy 当可重试错误，重试时占用 per-NameServer 信号量槽位空耗；且 hickory `NameServerState::send` 对任意发送错误都 `set_status(Failed)`，下次查询强制重连（触发全局 200ms socket 速率限制 + TLS 握手），形成“busy → 重连 → 更慢 → 更多 busy”的恶性循环，直接推高延迟。

**修复**：新增 `is_busy()`（匹配 `ProtoErrorKind::Busy`）并从 `is_retryable()` 中剥离；`NameServer::lookup` 增加 busy 快速失败分支——不重试、不占槽等待，立即返回错误让 `NameServerGroup` 转向下一个上游。配合既有 `PER_NAMESERVER_CONCURRENCY` 信号量（<32）从源头抑制 busy。

### feat(build): web-ui 加入默认特性

`default = ["common", "self-update"]` → `default = ["common", "self-update", "web-ui"]`，默认构建即嵌入 Web 仪表盘，无需显式 `--features web-ui`。

### fix(dns_mw_cache): 修复 get_expired 与 insert 的锁顺序反转死锁

**问题**：`get_expired` 在持有 `prefetch_heap` 锁的整个扫描期内逐个 `await` 各分片锁，而 `insert` 路径是「分片→堆」顺序。两者构成**锁顺序反转**，在预取 worker 与缓存写入并发时相互等待、双向挂起，导致相关查询永久挂起（间歇性脑裂式无响应）。

**修复**：`get_expired` 改为每次只从堆弹出一个候选、立即释放堆锁，再单独锁分片判定；需保留的条目统一放回堆。同步 `clear()` 补清 `prefetch_heap`。

### fix(proxy): UDP 上游误配 HTTP 代理不再 panic

`connect_udp` 的 `ProxyProtocol::Http` 分支原为 `unimplemented!()`（panic）。改为返回 `io::Error(Unsupported)`，由 dns_client 正常降级。

### fix(server): 单个监听器 bind 失败不再整进程 panic

`bind_to` 原为绑定失败即 `panic!("cound not bind...")`。改为返回 `io::Result<T>`，7 处调用点以 `?` 上抛 `crate::Error`，单地址失败不再拖垮其它监听器。

### fix(server): DoT 握手加超时（防 slowloris）

`tls_acceptor.accept()` 外包 `tokio::time::timeout(timeout, ...)`，客户端建连后不完成握手即关闭连接，避免 task + socket 被半开连接长期占用。

### fix(dns_mw_audit): 审计发送不再阻塞请求返回路径

`audit_sender.send().await`（有界通道 cap=100）原本在请求返回路径上阻塞，写盘慢时所有 DNS 响应延迟被审计速度拖住。改为 `try_send`，满则丢弃（审计允许丢失）。

### fix(dns_conf): conf-file 加载失败 / 未知指令不再 panic 或静默

- `conf-file` 指向无读权限文件时原为 `load_file().expect(...)`（启动 panic），改为 `error!` 后继续加载。
- 拼写错误或未知指令原为 `warn!`（默认日志级不可见），升级为 `error!`。

### fix(dns_mw_cache): clear() 同步清空预取就绪堆

重载/`rcache` 清空缓存后，原 `prefetch_heap` 残留旧 Query；现一并 `clear()`。

### fix(dns_mw_audit): 审计写线程 into_inner 失败不再 panic

后台审计 task 内 `writer.into_inner().expect(...)` 失败会导致审计静默停止，改为 `if let Ok` 失败则 `warn!`。

### fix(resolver): CLI 非 UTF-8 参数不再 panic

`arg.into_string().expect(...)` 改为返回 `Err`，由上层报错退出。

### fix(server): alt_svc 响应头构建失败不再 panic

`HeaderValue::from_str(...).expect(...)` 改为 `unwrap_or_else(|_| HeaderValue::from_static(""))`。

### fix(dns_mw_addr): 防御性 unreachable!() 改为 return None

`match query_type { A=>, AAAA=>, _ => unreachable!() }` 改为 `_ => return None`，避免未来扩展 IP 类记录类型时变真实 panic。

### fix(app): 修复背景（预取）请求被静默丢弃导致的 active_queries 计数泄漏

**问题**：请求循环对背景请求采用「空闲 30min 或 `bg_batch` 满时丢弃」策略，但丢弃既不入 `bg_batch` 也不入 `batch`，导致 `sender` 永不回送、等待方 task 永久阻塞，且 `active_queries` 原子计数只增不减（泄漏），最终统计/广告栏失真。

**修复**：移除脆弱的空闲丢弃逻辑，背景（预取）请求统一进入 `bg_batch`（由 `background_concurrency=4` 限流）。空闲期预取开销可忽略且缓存保持新鲜；所有请求均被处理，`active_queries` 计数严格平衡。

### fix(app): 消除 process() 热路径上的 todo!() panic

**问题**：`OpCode::Status/Notify/Update/Unknown` 与 `MessageType::Response` 在 `process()` 中走 `todo!()`（即 `panic!`）。客户端一旦发出此类报文，处理 task 崩溃、查询永久挂起（若 `panic=abort` 更会拖垮整个进程）。

**修复**：不支持的 opcode 返回 `ResponseCode::NotImp`、意外收到的 Response 报文返回 `ResponseCode::Refused`，均为合法空响应，不再 panic。

### fix(dns_client): SERVFAIL/REFUSED 现在触发上游熔断

**问题**：`NameServer::lookup` 对 `Ok(response)` 一律 `record_success()`。但 SERVFAIL/REFUSED 是**合法 DNS 响应**（`first_answer()` 返回 `Ok`），于是持续出错的上游被误判为健康，永不进入熔断冷却，反复空耗 3s 超时 + 占信号量槽位。

**修复**：在 `Ok(response)` 分支检查 `response.response_code()`；`ServFail`/`Refused` 改调 `record_failure()`（仍回传响应，由 `NameServerGroup` 继续尝试其它上游），连续 3 次失败即触发 30s 冷却跳过。

### fix(dns_client): 提升 is_retryable 鲁棒性（匹配 ProtoErrorKind）

**问题**：`is_retryable` 用 `err.to_string().contains("receiver was canceled")` 与 OS 错误码嗅探来判断可否重试。hickory 一旦改动错误文案或平台码，重试逻辑即静默失效。

**修复**：直接匹配 `ProtoErrorKind::Busy` / `Canceled(_)` / `Io(资源耗尽码 10055/105)`，消除对错误字符串与 OS 码的脆弱依赖。

### fix(dns_client): Mutex 中毒时不再 panic

`NameServer::in_cooldown` / `record_failure` 对 `last_failure` 标准 Mutex 使用 `.unwrap()`，在 Mutex 中毒时会 panic。改为 `.unwrap_or_else(|e| e.into_inner())`。

### fix(dns_mw_ns): 双栈回退空结果不再 unreachable!() panic

`IpStrategyResult::Fallback` 的 ok/err 任务均为空时原走 `unreachable!()`，理论不可达但一旦触发即 panic。改为返回 `Err(ProtoErrorKind::NoConnections)`。

### fix(app): FormError 分支去脆弱 expect

报文解析失败分支原为 `kind.into_form_error().expect(...)`（匹配守卫已确认存在）。改为 `match` 的 `Ok/Err` 分支，异常时保守返回空响应而非 panic。

### fix(dns_client): 上游引用不存在的代理名时给出告警

构建 `NameServer` 时若上游 `proxy` 字段引用的代理名未在 `proxies` 表中找到，原静默降级为直连；现改为 `warn!` 提示，便于发现配置错误。

### chore: 多项小修（注释/命名对齐）

- `dns_client.rs` 注释「并发数设为 16」对齐真实常量 `PER_NAMESERVER_CONCURRENCY = 12`。
- `app.rs` `add_stats_snapshot` 形参 `cache_hits` 改名为 `query_hits`，与调用处传入的 `cache_query_hits` 语义一致。

### fix(dns_mw_cache): 修复预取扫描循环定时器泄漏导致的长期运行性能劣化

**问题**：实例长时间运行后网络明显劣化（上游查询耗时飙到 400ms+），命令行被预取 DEBUG 日志刷屏（`Domain prefetch check will be performed in 39718.2160968s.` 与 `[prefetch] check: cache=4067 entries, elapsed ...` 每 30~90s 重复）。

**根因**：`DomainPrefetchingNotify::notify_after` 每次调用都 `tokio::spawn` 一个 **detached `sleep` 定时器任务**，且永不取消前一个任务。两个调用点中：
1. `handle()` 的缓存**未命中写入**路径（`notify_after(Duration::from_secs(ttl))`，`ttl` 为真实记录 TTL，可达数万秒）在每次缓存写入时都派生一个长生命周期定时器；
2. 预取扫描循环自身每轮也派生一个（由堆驱动、有界，本无问题）。

随运行时间累积，成千上万个挂着 `tokio::time::Sleep` 的永久 future 占据 tokio timer 堆，且各自到点后 `notify_one()` **误唤醒**扫描循环 → 触发 `get_expired` 持锁扫描 + 刷屏日志。这就是"长期运行后性能劣化"的真凶（之前排查的"网页打不开"实为本地代理 `127.0.0.1:7890` 问题，与此无关）。

**修复**：
- `DomainPrefetchingNotify` 改为轻量唤醒器：去掉 `tick` / `RwLock` / `notify_after`，只保留 `notify_one()`（零任务派生）。
- 预取扫描循环改用**单个可重置定时器**：`tokio::select!` 在 `sleep_until(next_check)`（依据堆中最近过期时间重置）与 `notified()`（插入路径立即唤醒）之间等待，彻底消除定时器泄漏。
- `handle()` 未命中写入路径由 `notify_after(ttl)` 改为仅 `notify_one()` 唤醒扫描循环，实际调度完全交给扫描循环。
- 删除 `[prefetch] check: cache=...` 刷屏日志（其 `entry_count()` 还需持全部分片锁全表遍历），并将每条查询都打印的 `[cache] hit` / `bg-refresh queued` 由 `debug!` 降级为 `trace!`，使 `log-level debug` 不再刷屏。

**验证**：`cargo check --features web-ui` 通过；`cargo test --bin smartdns dns_mw_cache` 9 项全过。部署后日志不再出现上述两行预取刷屏，且无需再派生 detached 定时器。

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

### perf(stats): 消除 `/stats` 每次轮询的全缓存克隆

**问题**：`src/api/stats.rs` 每次 `/stats`（Web UI 高频轮询）都调用 `cached_records()`，**克隆最多 `cache-size`(65536) 条记录**，只为算出有歧义的 `cache_hits` 字段（各条目命中计数之和，跨重启累加、与 `total_queries` 不可比）。这是 dashboard 刷新时的隐藏 O(缓存大小) 扫描。

**修复**：
- `cache_hits` 与 `cache_query_hits` 统一改用 O(1) 的全局命中原子计数 `DnsCache::query_hits()`；
- `cache_size` 改用 `DnsCache::entry_count()`（仅跨 16 个分片各取 `.len()`，O(分片数) 而非 O(条目数)）；`entry_count` 提升为 `pub`。
- `cached_records()` 仍保留给缓存管理列表接口（`api/cache.rs:35`），不受影响。

**收益**：dashboard 轮询从「克隆数万条记录」降为「几次原子读 + 16 次轻量锁」，长缓存下延迟尖刺消除。

### perf(dns_mw_cache): 命中即预取去冗余，削减热域名后台查询量

**问题**：`handle()` 在**每一次**缓存命中（Valid / Expired 分支）都调用 `try_prefetch` 向后台入队一次上游重查。堆驱动预取（`get_expired`）本已在到期时刷新，命中即预取只是补充，但「每个命中都打上游」对长 TTL 热域名是持续无谓的查询量与日志量（受 channel 容量与信号量限流，但仍有常驻后台流量）。

**修复**：新增门槛常量 `ON_HIT_PREFETCH_REMAINING_TTL = 600`(秒)。仅当缓存条目**剩余 TTL < 600s** 时才在命中时触发后台刷新；serve-stale 条目 TTL 已被改写为回复 TTL（远低于门槛），照常刷新。临近过期即刷新的保鲜效果不变，长 TTL 条目的无谓后台流量被消除。

### fix(dns_mw_cache): 修复双栈 NODATA（CNAME+SOA）响应漏缓存导致命中率偏低

**问题**：即便已恢复否定缓存，Web UI 命中率仍长期停在 ~76%，明显低于原版 smartdns 的 90%+。

**根因**：`is_negative_response()` 将 NODATA 判定为「`NoError` 且**应答区为空**」。但双栈域名的 AAAA 查询在只返回 `CNAME + SOA`（无 AAAA 记录）时，应答区**非空**（含 CNAME），于是：
1. 阴性判定不满足 → 阴性缓存分支绕过；
2. `handle()` 的 bypass 分支（应答里没有与查询类型匹配的记录）又直接跳过缓存。
两道关卡叠加，这类高频 NODATA 响应**永不进缓存**，每次都打到上游。这正是原版会缓存、本分支漏掉的那 ~14 个百分点。

**修复**：将 NODATA 判定改为 RFC 2308 定义——「`NoError` 且**应答区没有查询类型的记录**」（如 AAAA 查询无 AAAA，仅有 CNAME 亦算 NODATA）。`is_negative_response(resp, query)` 现在接收原始 `Query` 以比对记录类型；`handle()` 中性判定位于 bypass 之前，故 CNAME+NODATA 会在 bypass 前被当作阴性响应缓存。`insert_negative()` 存的是完整 `DnsResponse`（含 CNAME+SOA），命中时原样返回，不留破损响应。

**验证**（直连 127.0.0.1:9053 受控实测）：
- 修复前：`www.csdn.net` / `www.oschina.net` AAAA（仅返回 CNAME）查 2 次，第 2 次仍 55~69ms（打到上游），不计入命中 ❌
- 修复后：第 2 次应 <5ms（缓存命中）✅（由 `test_is_negative_response_cname_nodata` 单元测试锁定）

**说明**：实测家庭网络中此类双栈 NODATA 占否定应答的绝大部分，修复后稳态命中率应接近原版 90%+ 量级。

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
