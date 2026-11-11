# smartdns-rs 性能/开销专项审计报告（2026-11-11）

> 本报告为 2026-10-25 全量审计后的**增量审计**，聚焦热路径性能与开销（内存分配/CPU/延迟）。
> 基线：上游合并后 HEAD（含 TCP pipelining 修复 + IPv6 邻居缓存）。

## 一、审计范围与方法

- 方法：静态代码审查（全部中间件 + server 入口 + 缓存 + 上游客户端 + 连接层），结合 Explore agent 热路径逐行调查
- 量化模型：以"每请求固定开销"为标尺（UDP 请求、缓存命中、默认配置场景）

## 二、热路径每请求开销清单（现状）

### A. 量级最大的确定性开销（建议优化）

| # | 位置 | 问题 | 量级 |
|---|------|------|------|
| A1 | `src/dns_mw_audit.rs:92` | 审计中间件每请求 `res.clone()` 深拷贝**整个 DnsResponse（含完整 Message + 所有 Record Vec）**，即使未启用磁盘审计、仅填充 WebUI 环形缓冲 | 每请求 1 次全量 Message 克隆 |
| A2 | `src/app.rs:541` | `let qname = request.query().original().to_string()` 在 `log::debug!` **之前**无条件执行，即使日志级别为 WARN 也发生 | 每请求 1 次 Name→String 格式化分配 |
| A3 | `src/dns_mw_cache.rs:1035` | 缓存命中路径 `value.data.clone()` 深拷贝整个 DnsResponse | 每次缓存命中 1 次全量 Message 克隆 |

### B. 结构性开销（改进收益中等或需权衡，暂不动）

| # | 位置 | 问题 | 量级 |
|---|------|------|------|
| B1 | `src/collections.rs:16-82` | `DomainMap::find` 每次 2 次 Name 堆分配 + 按域名层级 O(labels) 次 Name 全量重哈希（无 hash 缓存、无 trie） | 每请求 1-2 次 find |
| B2 | `src/server/mod.rs:302` | `ServerOpts` 每请求 clone ×2-3（13 字段栈拷贝 + 有值时 2 String 堆分配） | 每请求 2-3 次 |
| B3 | `src/infra/middleware.rs:73-80` | 中间件链每层 `next.run()` `.boxed()`，全链 ~10 层 | 每请求 ~10 次 BoxFuture |
| B4 | `src/dns_mw_ns.rs:87/170` | `server_group_name().to_string()` ×2 | 每请求 2 String |
| B5 | `src/dns.rs:218-226` | 内部路径（prefetch/dualstack）`SerialMessage::Raw` 跨通道时 Message clone | 每次内部查询 1 次 |
| B6 | `src/dns_mw_cache.rs:870-896` | 缓存 insert 路径 `lookup.clone()` ×2 + `query.clone()` + String ≈ 5+ 次分配 | 每次缓存写入 |
| B7 | `src/dns_mw_dualstack.rs:92-100` | dualstack 触发时 `ctx.clone() + req.clone() + next.clone()` + 两族并发跑完整中间件链（双倍下游开销） | 每次触发 |
| B8 | `entry_size`（dns_mw_cache.rs:1188） | 每次 insert/evict 调 `to_utf8()` String 分配估算内存，无条件执行 | 每次缓存写入/淘汰 |

### C. 已确认良好（无需优化）

- **ICMP client 复用**：`src/infra/ping.rs:422-440` 全局静态复用（commit 86eb4e7b）✅
- **ping 触发模型**：仅 cache-miss 时 ping，缓存命中不 ping ✅
- **socket 限速器**：`resource_busy_recently()` 仅在最近 60s 出现 WSAENOBUFS/ENOBUFS 才启用 200ms 节流，稳态零 sleep ✅
- **缓存持久化**：后台周期 checkpoint（默认 24h）+ 退出时，**不在查询路径写盘** ✅
- **审计写盘**：有界 channel(100) + 满即丢弃，不阻塞请求返回路径 ✅
- **tracing 宏**：惰性求值，宏参数安全 ✅
- **prefetch 唤醒**：已改为零任务派生（dns_mw_cache.rs:535-560）✅

## 三、本次执行的优化（按收益/风险排序）

### 优化 1（A1）：消除审计中间件每请求全量 Message 克隆

**问题**：`DnsAuditMiddleware::handle` 每请求构造 `DnsAuditRecord { result: res.clone(), .. }`，深拷贝整个 DnsResponse。而消费端事实是：
- WebUI 环形缓冲（`QueryLogEntry::from`）只用 `result.is_ok()` 和记录数
- 磁盘审计（文件/syslog/console）仅在启用时需要，且走异步通道，可延后格式化

**改动**：
- `DnsAuditRecord.result` 从 `Result<DnsResponse, DnsError>` 改为轻量摘要 `result_summary: Result<usize, ()>`（记录数或错误态）
- `QueryLogEntry::from` 不再依赖完整 result
- 磁盘审计文件路径保留原始数据（仅当 audit 文件/syslog 启用时才在 handle 中 clone 一次，写入有界通道）

### 优化 2（A2）：消除 app.rs 无条件 qname String 分配

**问题**：`app.rs:541` `qname = request.query().original().to_string()` 无条件执行，仅为后续 3 处 `log::debug!` 服务；默认日志级别 WARN 下纯属浪费。

**改动**：删除提前的 `to_string()`，在各个 `log::debug!` 宏内直接使用 `request.query().original()`（tracing 惰性求值保证 level 未启用时零开销）。

### 优化 3（B8 部分）：entry_size 免 String 分配

**问题**：`entry_size` 用 `to_utf8()` 生成 String 只为取 `.len()`。

**改动**：改用 `Name::len()`（hickory 编码长度）或先算 UTF-8 长度。hickory `Name` 无直接 utf8 长度方法，改为遍历 labels 求和（label.len()+1），零分配。

## 四、不建议本次动的（留档）

- **A3 缓存命中 clone**：需要把缓存值改为 `Arc<DnsResponse>` 才能消除，涉及 DnsCacheEntry 泛型、CNAME 改写逻辑（get 路径会原地改写 record name）、快照导出等多处行为变化，风险大于收益，留待独立 PR。
- **B1 DomainMap hash 缓存/trie**：experimental-trie feature 存在但未默认开启，属上游架构演进。
- **B3 BoxFuture**：为中间件机制固有，消除需重构为 enum dispatch。
- **B7 dualstack 双倍下游开销**：属架构设计（两族竞速），非 bug。

## 五、验证计划

1. `cargo check` + `cargo test`（全量）
2. 对比合并前基线：唯一预存失败 `test_stress_multi_bind_udp_with_protection`（与本次改动无关，合并前已失败）
3. 审计输出格式回归：dnsmasq/tcp/audit 相关单元测试全绿

---
*报告生成：WorkBuddy 项目审计（project-audit）· 2026-11-11*
