# AUDIT REPORT — 2026-10-25

对象：`smartdns-rs`（Rust 移植版，基线 `v0.13.5` / `8dd30c6`）对 `pymumu/smartdns`（C 原版，`dfc9ef4`）。
方法：XERJ 双侧索引（`--prefix sdrs` / `--prefix smartdns-c`）+ 直接读两侧源码 + 机械扫描（panic 标记、
非测试代码 `unwrap/expect`、逐文件测试数、文件体量、unsafe、API 路由与鉴权）。
本报告只记录发现与建议，不含任何代码改动。

> 存放位置说明：项目用的是 `docs/` 而非 `doc/`，故落盘到 `docs/`。

---

## 1. 现状描述（可观察事实）

| 项 | 数值 |
| :--- | :--- |
| Rust 源文件 / 行数 | 167 个 / 33 542 行（`find src -name '*.rs'`） |
| 单元测试 / 集成测试 | 362 个 `#[test]`+`#[tokio::test]`；`tests/` 3 个文件、18 个用例 |
| 零测试源文件 | **79 / 167**（含 `src/api/*` 全部 14 个、`src/app.rs`、`src/dns_mw_dualstack.rs`、`src/server/*` 7 个） |
| 最大文件 | `dns_conf.rs` 2 510、`dns_mw_cache.rs` 1 811、`dns_client.rs` 1 439、`dns_url.rs` 975、`infra/ping.rs` 921 |
| 非测试代码 `todo!/unimplemented!` | 11 处，集中在 Linux 服务后端（`service/linux/*`）、Windows 未使用分支（`dns_url.rs:261/536`、`libdns/mod.rs:63`、`infra/ipset.rs:108`） |
| `unsafe` | 10 处，全部在 `ffi/nftset*` 与 `infra/mem_bytes.rs` |
| 门禁 | `cargo test` 343+10+7 全绿、clippy 0、fmt 0；CI 19 job 全绿；发布链路 GitHub Release 正常（crates.io 不可行，见 AGENTS.md） |

分层：`config/`（parser + schema）→ `dns_*`（数据结构、客户端、中间件）→ `server/`（各协议监听）→ `api/`（Web UI + HTTP DNS）→ `libdns/`（hickory 兼容层）→ `infra/`（ping/dhcp/进程/内存映射）。

---

## 2. 问题清单

### 2.1 安全

| # | 位置 | 问题 | 严重度 | 影响范围 |
| :--- | :--- | :--- | :--- | :--- |
| S1 | `src/api/*.rs`（无 auth 中间件）；`POST /api/caches/flush`、`POST /api/config/reload`、`POST/PUT/DELETE /api/addresses` | HTTP 管理接口**无任何鉴权**，且包含改状态/改规则/重载配置的写接口。发行默认绑定 `127.0.0.1`，两份 README 也都已警示「面板无鉴权、仅绑回环」，因此属**已声明的设计取舍**；风险出现在用户自行改成 `bind-http :8083`（监听所有网卡）时 | 中（默认安全，取决于部署） | 任何能访问该端口的设备可清空缓存、增删 address 规则、触发配置重载 |

### 2.2 正确性 / 健壮性

| # | 位置 | 问题 | 严重度 | 影响范围 |
| :--- | :--- | :--- | :--- | :--- |
| C1 | `src/dns_mw.rs:43-50` | 客户端 EDNS Client Subnet 的 `source_prefix` 直接被 `Ipv4Net::new/Ipv6Net::new(..).unwrap()` 使用。hickory 的 `EcsOption::read`（`crates/proto/src/rr/rdata/opt.rs:701-730`）把 prefix 当未校验 `u8` 读入，因此发包方给出 `source_prefix > 32`（v4）/`> 128`（v6）时 `new()` 返回 `Err` → **请求任务 panic** | 中 | 任意客户端可远程触发；tokio unwind 不会杀进程，但该查询不应答、每包刷一条 panic 日志；release 未开 `panic=abort`，故不会整进程崩溃 |
| C2 | `src/dns_mw_cache.rs:423-434`（入库）、`src/dns_mw_addr.rs`（回复） | 审计初稿把链序写反了：`builder.with` 先注册者最外层，addr 在 cache **外层**，所以 `rr-ttl-reply-max` 本来就只压回复、**与 C 一致**。真正的分歧是 `rr-ttl` / `rr-ttl-min` / `rr-ttl-max`：C 在**入库时**就用 `_dns_server_get_conf_ttl` 计算缓存 TTL（`cache.c:140`），Rust 只在回复时夹取，缓存存的是上游原始 TTL ——短 TTL 记录会比配置的最小值更早过期，长 TTL 记录会突破 `rr-ttl-max` 继续驻留 | 已核实并修复 | 开启 `rr-ttl-min` 时缓存命中率偏低、上游查询偏多；开启 `rr-ttl-max` 时可能超期提供陈旧记录 |
| C3 | `src/infra/mapped_file.rs:250,254` | `Mutex::lock().unwrap()`：写线程一旦 panic 使互斥量中毒，后续所有持久化写入连锁 panic | 低 | 缓存持久化写入路径；进程仍可服务，但 checkpoint 失效且刷错误日志 |
| C4 | `src/infra/dhcp.rs:58` | `M::from_bytes(&buf[..n]).unwrap()` 解析收到的 DHCP 报文 | 低 | 需确认调用点是否在在线请求路径；畸形报文会 panic 该任务 |
| C5 | `src/config/domain.rs:175` | `Display` 里 `str::from_utf8(self.0.pattern()).unwrap()` | 低 | 仅在 wildcard 模式可打印时；构造来源是 `&str`，当前不可达 |
| C6 | 配置面 | `bind-cert-key-pass` 解析入配置但加载私钥时未使用（已在 `docs/port-gap-analysis.md` §22.9、`TODO.md` 记录） | 低 | 用户以为加密私钥可用，实际被忽略 |

### 2.3 测试覆盖（维度 5 / 6）

| # | 位置 | 问题 | 严重度 | 影响范围 |
| :--- | :--- | :--- | :--- | :--- |
| T1 | `src/app.rs`（825 行） | 中间件链装配、错误到响应码的映射无单测；`max-query-limit` 判定已抽出 `should_refuse_query` 并有 4 条用例（**判定已覆盖，真实链路的并发行为仍未测**） | 中 | 门限语义有回归网；链序仍需人工压测 |
| T2 | `src/dns_mw_dualstack.rs` | 阈值判据已抽出 `is_that_faster` 并覆盖 4 个边界；**ping 结果无法注入**，`which_faster` 与 `multi_mode_ping_fastest` 仍依赖真实 ICMP（CI 关闭），完整选择策略未测 | 中 | 判据有回归网；真实优选行为仍只能线上观察 |
| T3 | `src/server/{udp,tcp,tls,https,quic,h3}.rs` | 所有监听器的 bind/超时/错误路径无单测；只靠 CI 编译与手工联调 | 中 | 端口/协议层回归依赖人工 |
| T4 | `src/api/*.rs`（14 文件） | 0 测试，含 4 个写接口 | 中 | 接口语义变化无回归网 |
| T5 | `src/libdns/custom/connection_provider.rs` | 连接池/限速器有 `test_stress_*` 用例但耗时 >60s，且核心分支（槽位耗尽、取消）靠压测而非确定性用例 | 中 | 僵尸请求类问题复发难以及早发现 |
| T6 | `tests/` | 仅 3 个文件 18 例，缺「启动服务 → 真实 DNS 报文 → 断言中间件链输出」的端到端用例 | 中 | 链序/开关组合的集成回归无覆盖 |
| T7 | `src/dns_mw_bogus.rs`、`infra/process_guard.rs`、`infra/http_client.rs` | `bogus-nxdomain` 已补 1 条链路用例；`process_guard` / `http_client` 仍零测试 | 低 | 局部逻辑错误只能靠线上暴露 |

### 2.4 文档一致性 / 缺漏（维度 1 / 2）

| # | 位置 | 问题 | 严重度 | 影响范围 |
| :--- | :--- | :--- | :--- | :--- |
| D1 | `README.md:108`、`README_zh-CN.md` | 称预取批量「可配置（默认 5 域名）、最小间隔默认 500ms」，实际这两个值只能通过**环境变量** `PREFETCH_MAX_BATCH` / `PREFETCH_MIN_INTERVAL` 调（`src/dns_mw_cache.rs:140-151`），不是配置文件选项，也不在 `/api/config` 可见 | 低 | 用户按 README 去 conf 里找开关找不到 |
| D2 | `README.md:292-294`、`README_zh-CN.md` 同名段落 | 「面板无鉴权、仅绑回环」的提示**已存在**（实测核对）；缺的是**可写接口清单**（`POST /api/caches/flush`、`POST /api/config/reload`、`POST/PUT/DELETE /api/addresses`）与「反向代理 + 认证」的推荐做法 | 低 | 用户知道面板无鉴权，但未必意识到接口能改规则/重载配置 |
| D3 | `README.md:46,350` | 声称 32 槽缓冲区——**与代码一致**（`dns_client.rs:48` 注释 `CHANNEL_BUFFER_SIZE(32)`），无需改 | — | 记录为已核对项 |
| D4 | `docs/port-gap-analysis.md` | 与 C 原版的指令覆盖率、逐项启用建议完整；本次复核未发现新的过期条目 | — | — |

### 2.5 架构与工程（维度 3 / 7）

| # | 位置 | 问题 | 严重度 | 影响范围 |
| :--- | :--- | :--- | :--- | :--- |
| A1 | `dns_conf.rs` 2 510 行；`dns_mw_cache.rs` 1 811 行；`dns_client.rs` 1 439 行 | 单文件同时承载 schema、解析后处理、运行时行为；新增指令要改 3 处以上，易漏 | 中 | 可维护性；新增配置项成本高 |
| A2 | `src/dns_mw_cache.rs:140-151` | 用环境变量做运行时调参，与项目「配置项集中在 parser + RuntimeConfig」的体系不一致，测试也难注入 | 低 | 可测试性/可观测性 |
| A3 | `justfile` / CI / 版本号 / 打包 | 已核对：15 目标构建、聚合 release、SHA256SUMS、CHANGELOG 段落注入均正常；`just` 非本机依赖已在 AGENTS.md 说明 | — | 无需改动 |

---

## 3. 改进建议（可直接开工）

### 建议 1（对应 C1，优先级 P0，可并行）
**做法**：把 ECS → `IpNet` 的转换收敛成返回 `Option<IpNet>` 的小函数；`source_prefix` 超出地址族位数时返回 `None`（等价于未携带 ECS，回落客户端源 IP），不再 `unwrap`。C 版同样在解析阶段拒绝非法 prefix。
**文件**：`src/dns_mw.rs`（转换点 43-50 行 + `mod tests` 新增用例）。
**验证**：新增单测覆盖 `prefix=32/128`（合法）、`prefix=33/129`（非法返回 None）、`::ffff:a.b.c.d` 映射；`cargo test --bin smartdns dns_mw`；clippy 零告警。

### 建议 2（对应 C3，优先级 P1，可与建议 1 并行）
**做法**：`mapped_file.rs` 两处 `lock().unwrap()` 改为 `lock().unwrap_or_else(|e| e.into_inner())`（与 `dns_client.rs` 已采用的毒性恢复写法一致）。
**文件**：`src/infra/mapped_file.rs`。
**验证**：`cargo test --bin smartdns mapped_file`；构造中毒场景可选用例（可选）。

### 建议 3（对应 C2，优先级 P1，需串行在建议 1/2 之后）
**状态：已实施**。链序经代码核对为 addr（外层）→ cache（内层），`reply-max` 无需搬家；实际修的是
`rr-ttl` / `rr-ttl-min` / `rr-ttl-max` 未作用于缓存 TTL：抽出 `rr_ttl_bounds` / `clamp_ttl_bounds`
（`src/dns_mw_addr.rs`），缓存中间件入库前按同一套边界夹取，`reply-max` 保持只压回复。
**验证**：新增 2 个单测（边界折叠含 `rr-ttl` 固定值与 `reply-max` 不参与、夹取顺序）；全量测试 348+10+7 绿。

### 建议 4（对应 S1 + D2，优先级 P0-文档 / P1-实现，可分两步）
**做法**：① 在现有警示下方补两句：列出可写接口（缓存清空、address 增删改、配置重载），并给出「放反向代理 + 认证之后」的推荐做法。② 中期可选：为 `/api/*` 增加 token 或 basic auth（配置项 `web-ui-auth` 之类），写接口强制鉴权。
**文件**：README 两份；实现阶段涉及 `src/api/mod.rs`、`src/config/parser/*`、`src/app.rs`。
**验证**：文档改动无需测试；实现阶段补接口鉴权用例（401/200 两路）与 `test_parity_directives` 风格解析测试。

### 建议 5（对应 T1/T2/T6，优先级 P1，测试分支）
**做法**：优先补三条确定性用例：`app.rs` 的 `max-query-limit` 门（并发 N+1 时第 N+1 个回 REFUSED）、`dns_mw_dualstack` 的阈值分支（`ping` 结果注入桩）、`tests/` 里一条「UDP 报文进 → 命中 address 规则 → 断言应答」的端到端用例。
**文件**：`src/app.rs`、`src/dns_mw_dualstack.rs`、`tests/integration_*.rs`。
**验证**：新增用例在 CI 三平台（test 矩阵）通过；不引入 `sleep` 竞态（用暂停时钟或注入桩）。

### 建议 6（对应 A1/A2，优先级 P2，架构分支，需单独一轮）
**做法**：把 `dns_conf.rs` 的后处理按子系统拆成 `config/{cache,audit,log,nameserver}.rs` 的 `apply()`；环境变量调参改为配置项（或至少收敛到 `RuntimeConfig` 读取一次）。
**文件**：`src/dns_conf.rs`、`src/config/*`、`src/dns_mw_cache.rs`。
**验证**：纯重构，行为不变——以现有 362 个测试 + shipped conf 用例为回归基线；逐文件迁移、每步 `cargo test`。

### 建议 7（对应 D1，优先级 P3，可并行）
**做法**：README 把「configurable batch」改为「环境变量可调，默认 5/500ms」，并说明不能通过 conf 配置。
**文件**：`README.md`、`README_zh-CN.md`。
**验证**：与 `dns_mw_cache.rs:140-151` 逐字对齐。

---

## 4. 优先级与并行性

| 序 | 条目 | 优先级 | 并行性 | 涉及文件是否重叠 |
| :--- | :--- | :--- | :--- | :--- |
| 1 | 建议 1（ECS prefix panic） | P0 | 可并行 | `dns_mw.rs` 独占 |
| 2 | 建议 2（mutex 毒性） | P1 | 可与 1 并行 | `mapped_file.rs` 独占 |
| 3 | 建议 4-①（README 安全警示） | P0-文档 | 可与 1/2 并行 | 两份 README |
| 4 | 建议 7（预取参数措辞） | P3 | 可与 3 并行（同文件，需串行合并） | 两份 README |
| 5 | 建议 3（`rr-ttl` / `rr-ttl-min` / `rr-ttl-max` 入库生效） | 已完成 | — | `dns_mw_addr.rs`、`dns_mw_cache.rs` |
| 6 | 建议 5（补测试） | P1 | 可在 3 之后并行 | `app.rs`、`dualstack`、`tests/`（与 3 的 `app.rs` 冲突 → 串行） |
| 7 | 建议 4-②（API 鉴权实现） | P2 | 需单独设计 | `api/*`、`config/*` |
| 8 | 建议 6（拆分大文件） | P2 | 需单独一轮 | `dns_conf.rs` 等 |

---

## 5. 本轮覆盖范围与未覆盖项（诚实声明）

- **已逐项对照 C 原版**：配置指令全集（此前 16 批已补齐并记录于 `docs/port-gap-analysis.md`）；TTL 截断的配置面（`rr-ttl-*` 在 Rust 的接线点已定位）；CNAME 规则链（Rust 用新 `DnsContext` 重入，不会递归应用 cname 规则，C 的 `has_cname_loop` 主要针对上游返回链，hickory 侧有自有深度限制）；README 的 32-slot / 预取参数等数值声明。
- **未做穷尽逐函数比对**：C 的 `dns_server/context.c` 全流程、EDNS 选项全集、DNSSEC 验证路径、DoH3/DoQ 的协议细节、`libdns/custom/connection_provider.rs` 的并发正确性——这些需要构造竞态/协议级用例，超出本次静态审计与 token 预算。
- **运行态**：本机部署 0.13.5 实测正常（见 `docs/` 之外的部署记录）；C2 已按代码（链序）与 C 原文（cache.c:140）定性并修复，未做真实上游 TTL 的端到端复现。

## 6. 处置状态（审计后同日更新）

| 条目 | 状态 |
| :--- | :--- |
| C1（ECS 前缀 panic） | **已修复**：`src/dns_mw.rs` 新增 `ipnet_from_subnet`，非法前缀返回 `None`（按未携带 ECS 处理），3 个单测覆盖合法/非法/IPv4-mapped |
| C3（写锁中毒） | **已修复**：`src/infra/mapped_file.rs` 两处改为 `unwrap_or_else(|e| e.into_inner())` |
| D1（预取参数措辞）／D2（可写接口清单） | **已修复**：两份 README 同步 |
| C2（`rr-ttl/min/max` 未作用于缓存 TTL；`reply-max` 链序原判断有误） | **已修复**：`rr_ttl_bounds` / `clamp_ttl_bounds` 供 addr 与 cache 共用，入库前夹取；新增 2 个单测 |
| S1（API 鉴权实现）、A1/A2（拆分大文件） | 未处置，按第 4 节优先级排期 |
| T1/T2 | **部分已补**：新增 `should_refuse_query`、`is_that_faster` 各 4 条边界用例（350+10+7 全绿） |
| T6（跨中间件端到端） | **受阻且已定位到一半**：直查 target 能命中 mock，但经 cname 改写后落到默认处理器；需要链路追踪（记录每层 middleware）再定位。详见 `docs/TEST-MATRIX.md` §4 |
