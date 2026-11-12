# smartdns-rs 全面代码审计报告

- 审计日期：2026-11-11
- 审计方式：只读审计，未修改任何项目文件
- 审计范围：全仓库（webui/node_modules、webui/out 产物、target/ 除外；webui/src 源码纳入审查）
- 审计基线：HEAD = `507455d6`（fix(webui): 修复 package-lock.json 与静态产物入库损坏（TSD 污染））

---

## 一、项目现状画像

### 1.1 规模统计

| 指标 | 数值 |
| --- | --- |
| git 入库文件总数 | 310 |
| Rust 源码（src/） | 168 文件 / 35,916 行 |
| WebUI TS/TSX 源码（webui/src/） | 10 文件 / 3,140 行 |
| 集成测试（tests/） | 2 文件 / 416 行 |
| src 子模块目录 | 9 个（api / config / ffi / infra / libdns / server / service / third_ext / zone） |
| 根级 Rust 文件 | ~25 个（dns_* 中间件、app、resolver 等） |
| 单元测试 | `#[test]` 332 + `#[tokio::test]` 25 ≈ 357 个（含 10 个 `#[ignore]` 网络用例） |
| 集成测试用例 | 18 个（stats_test 7 + cache_prefetch_test 11） |
| WebUI 测试 | 0（无 test runner、无 .test/.spec 文件、package.json 无 test 脚本） |
| GitHub Actions workflow | 7 个（build / test / nightly / perf / version / web-ui / wfcleanup） |
| 仓库 pack 体积 | 8.73 MiB |

### 1.2 目录结构概览

```
smartdns-rs/
├── src/                  # Rust 主 crate（~3.6 万行）
│   ├── api/              # Web UI + HTTP DNS API（15 文件，0 测试）
│   ├── config/           # 配置 schema + parser（nom，含 domain_set/adblock）
│   ├── infra/            # ping / dhcp / http_client / 进程管理
│   ├── libdns/           # hickory 兼容层（连接池/预热）
│   ├── server/           # UDP/TCP/TLS/HTTPS/QUIC/H3 监听
│   ├── service/          # 各平台服务管理
│   ├── zone/、ffi/、third_ext/
│   └── dns_mw_*.rs       # 中间件（addr/audit/cache/cname/dns64/...）
├── webui/                # Next.js 16 + Turbopack 前端（静态导出）
│   ├── src/              # 10 文件源码（dashboard 单页 + tabs/AdblockTab）
│   └── out/              # 构建产物 40 文件入库（RustEmbed 编译期依赖）
├── tests/                # 集成测试（2 文件）+ test_data 夹具
├── docs/                 # TEST-MATRIX、EVAL-ADBLOCK、历史审计报告
├── etc/smartdns/         # 示例配置
└── .github/              # 7 workflow + 2 composite actions
```

### 1.3 质量水平总评

项目是成熟度较高的生产型 fork：nom 解析器模式统一、中间件架构清晰、CHANGELOG/AGENTS.md/TEST-MATRIX 等工程文档异常完备（对个人项目而言罕见）。近期 adblock 功能实现质量总体良好：解析器有对称的 happy-path 测试、端到端覆盖「集合定义 → address 联动 → 例外放行」全链路、link_address_rule 有幂等设计、代码注释密度高且准确。

主要短板集中在三处：
1. **文档与功能脱节**——README / example.conf 对 adblock 与 domain-set 零提及，新功能对用户不可发现；
2. **新 API 层零测试 + API 错误语义粗糙**（500 代替 400、URL 静默吞错），WebUI 完全无质量门禁；
3. **双次下载的架构缺陷与 interval 假功能**——adblock 订阅同一 URL 下载解析两次；interval 字段全链路存在但运行时无消费方，WebUI 却让用户可编辑。

---

## 二、七维度逐条发现

> 每条含：位置 / 问题描述 / 严重程度 / 影响范围。共 39 项：高 5、中 17、低 17。

### 维度 1：文档一致性（5 项）

| # | 位置 | 问题描述 | 严重程度 | 影响范围 |
| --- | --- | --- | --- | --- |
| 1-1 | README.md、README_zh-CN.md（全文） | 两个 README 均未提及 adblock 功能与 domain-set 配置项（`grep -i adblock README*.md` 零命中、`grep -i domain-set README*.md` 零命中）。Web UI 章节只展示 overview/upstreams/cache 三张截图，无广告拦截 Tab 说明 | **高** | 所有用户：核心新功能不可发现 |
| 1-2 | etc/smartdns/example.conf:293-304 | domain-set 示例只写 `-type list`，未更新 `-type adblock` 用法；adblock 无任何面向用户的配置文档（仅 docs/EVAL-ADBLOCK-SUPPORT.md 内部评估文档） | 中 | 依找示例配置学习的用户 |
| 1-3 | CHANGELOG.md Unreleased 段（:10-26） | 变更描述与实现基本一致（E2E「969 条规则」与测试断言 >= 900 一致）；但所称「DELETE 后恢复解析」的 E2E 用例未在代码库中找到（疑为手工验证，未沉淀入库） | 中 | 变更记录可追溯性 |
| 1-4 | docs/TEST-MATRIX.md | 基线为 167 文件 / 33,754 行 / 370 测试（2026-10-26），当前已增至 168 文件 / 35,916 行，统计滞后 | 低 | 内部文档时效性 |
| 1-5 | README.md、CONTRIBUTING.md | 未说明编译 Rust（web-ui 特性默认启用）前需先 `cd webui && npm ci && npm run build` 生成 webui/out；原因仅记录在 .gitignore:64-70 注释中。CONTRIBUTING 对 webui/node 环境零提及 | 低 | 新贡献者首次构建 |

### 维度 2：文档缺漏（4 项）

| # | 位置 | 问题描述 | 严重程度 | 影响范围 |
| --- | --- | --- | --- | --- |
| 2-1 | src/config/domain_set.rs:187-273（语义仅存于代码注释） | adblock 语法支持范围无用户文档：支持 `\|\|domain^`、`@@\|\|domain^`、裸域名、hosts 行；跳过 `##`、`$` 修饰、`/regex/`、`\|http://`。用户无从得知哪些规则会生效、哪些被忽略 | **高** | adblock 功能可用性 |
| 2-2 | src/api/domain_set.rs:147-149 vs webui/src/app/dashboard/tabs/AdblockTab.tsx:243-244 | 「添加订阅即自动关联 `address /domain-set:<name>/#` 拦截」这一关键语义对用户不可见；AdblockTab 空状态提示反而让用户以为要手动配置 address 规则，与实际自动联动行为矛盾 | 中 | WebUI 用户预期管理 |
| 2-3 | 整体 | domain-set 机制（集合定义 vs address 规则引用的两段式设计）无架构文档 | 中 | 后续维护与贡献者 |
| 2-4 | AGENTS.md | 目录结构说明未覆盖 webui/ 子项目（Node 环境、构建流程、入库产物约定） | 低 | AI 协作与新人上手 |

### 维度 3：架构优化点（5 项）

| # | 位置 | 问题描述 | 严重程度 | 影响范围 |
| --- | --- | --- | --- | --- |
| 3-1 | src/config/domain_set.rs:112-132、:144-170 | `get_domain_set()` 与 `get_allow_domain_set()` 各自独立读文件/发起 HTTP 请求：adblock 订阅加载时同一 URL 被完整下载并解析**两次**（一次提 block 集、一次提 allow 集）。大列表（EasyList 数万条）双倍流量与解析耗时 | 中 | 订阅加载性能、网络流量 |
| 3-2 | src/api/domain_set.rs:20-26 | workaround 注释承认 utoipa_axum `routes!` 宏多 GET 会生成重叠路由，用两次 `into_router()` + `merge` 规避——第三方库限制，值得跟踪上游修复 | 中 | API 路由注册可维护性 |
| 3-3 | src/dns_conf.rs（2,664 行，全库最大文件） | 混合配置 schema、运行时构建（domain_set/allow_domain_map/domain_set_stats 装配 :997-1045）与 130+ 行测试；adblock 加载统计逻辑内联在 RuntimeConfig 构建中，与 config/domain_set.rs 的 provider 抽象职责割裂 | 中 | 可维护性 |
| 3-4 | src/config/domain_set.rs:46、src/api/domain_set.rs:215、AdblockTab.tsx:574-584 | `interval` 字段全链路存在（解析、序列化、WebUI 可编辑），但运行时**无任何消费方**（grep 全 src 仅 Display/parse/stats 展示）；订阅不会自动更新，仅重启/reload 重新拉取。EVAL 文档 :58 自认「该字段解析了但运行时未用」。WebUI 提供「自动更新间隔」输入框对用户构成功能存在但不生效的误导 | 低（架构）/ 高（用户误导） | 用户预期、功能完整性 |
| 3-5 | src/api/domain_set.rs:289-326 vs src/api/stats.rs | stats handler 直接 lock 查询日志环形缓冲聚合，与 api/stats.rs 同模式但各自实现，聚合逻辑可复用 | 低 | 代码重复 |

### 维度 4：代码写法（10 项）

| # | 位置 | 问题描述 | 严重程度 | 影响范围 |
| --- | --- | --- | --- | --- |
| 4-1 | src/api/domain_set.rs:202-218 | update handler：`input.url.as_deref().and_then(\|u\| Url::parse(u).ok())`——URL 解析失败时**静默 fallback 到旧 URL**，用户传入非法 URL 得到 204 成功但实际未更新。另 `input.interval.or(h.interval)`：清空 interval 框传 null 时 `null.or(old)` 保留旧值，UI 无法清除已有 interval，前后端语义错位 | **高** | 订阅更新正确性 |
| 4-2 | src/api/domain_set.rs:134-137、:187-191、:331-339 | 名称重复与未知 content_type 用 `ApiError::Internal(anyhow!(...))` 返回 500，应为 400 Bad Request 语义；list handler 中 `enabled` 字段恒为 true（:101 注释自认），是给前端预留的死字段 | 中 | API 语义正确性、前端错误处理 |
| 4-3 | webui/src/app/dashboard/tabs/AdblockTab.tsx:90/127/158/189 | `bgcolor: "error.main" + "22"` 字符串拼接生成半透明色，依赖 MUI 主题变量恰为十六进制的实现细节；page.tsx:115 的 `` `${color}20` `` 写法更稳。同类统计卡片在 AdblockTab 与 page.tsx MetricCard 两处重复实现（~130 行重复结构） | 中 | 主题可移植性、代码重复 |
| 4-4 | webui/src/lib/api.ts:263-282 | useUpdateDomainSet 内联裸 fetch（注释承认「apiPut 封装不存在」），与 apiGet/apiPost/apiDelete 封装风格不一致，应补 apiPut | 中 | 前端 API 层一致性 |
| 4-5 | AdblockTab.tsx:633-644 | EditSubscriptionDialog 在渲染期间调用 setState（`if (target && !initialized) { setUrl(...); ... }`）——React 官方反对的模式，虽因条件守护不会死循环，属反模式，应用 useEffect 或受控 key 重挂载 | 中 | React 渲染语义合规性 |
| 4-6 | src/dns_conf.rs:1021/1026/1033/1041 | 日志文案拼写错误 "DoaminSet"（应为 DomainSet），共 4 处 | 低 | 日志可检索性 |
| 4-7 | src/api/domain_set.rs:426-463 | link_address_rule 三个 match 臂重复 retain+write 块；(true, None) 分支 push 后才 retain，对新行必然通过，顺序绕但逻辑正确 | 低 | 可读性 |
| 4-8 | src/config/parser/domain_set.rs:66-69 vs :132-135 | File/Http 两个 provider parser 结构高度对称（~80% 重复），两处相同的 `Err(nom::Err::Error(...))` 构造，可抽公共 options 循环 | 低 | 解析器维护成本 |
| 4-9 | AdblockTab.tsx:502-507 vs :517-522 | `SubscriptionInput` 接口与 onSubmit 内联类型字段名不一致（contentType vs content_type）；EditDialog 传 `contentType: ""` 占位，类型建模混乱 | 低 | 类型安全 |
| 4-10 | AdblockTab.tsx:533 | 预置订阅 URL（AWAvenue）硬编码在组件内 | 低 | 配置可维护性 |

### 维度 5：测试覆盖率（5 项）

| # | 位置 | 问题描述 | 严重程度 | 影响范围 |
| --- | --- | --- | --- | --- |
| 5-1 | src/api/domain_set.rs（全文件 0 测试） | 整个 src/api 15 文件 0 用例（TEST-MATRIX G1 已记录），且本次新增 4 个写接口未补。create/update/delete 涉及托管文件读写（domain-set.conf、address.conf 联动）与热更新，是纯逻辑 + 文件 IO，本可用临时目录测 | **高** | 新 API 无回归网 |
| 5-2 | src/config/domain_set.rs、tests/test_data/ | adblock 解析核心覆盖尚可（parser 3 测试、解析器 3 测试、dns_conf 端到端 1 个、`#[ignore]` 真实订阅 1 个），但未覆盖：`domain.com$dnsrewrite` 类 `$` 修饰边界、`@@\|http://` 例外 URL 行、大小写混合域名、带端口 hosts 行、CRLF 行尾（Windows 订阅源常见） | 中 | 解析边界正确性 |
| 5-3 | webui/（整体） | WebUI 0 测试：无 eslint 配置文件（package.json 有 "lint" 脚本但无配置）、无 typecheck 脚本、CI webui job 只跑 `npm run build`（构建通过即绿） | 中 | 前端无质量门禁 |
| 5-4 | tests/stats_test.rs、tests/cache_prefetch_test.rs | 两个集成测试文件均不 import smartdns crate，各自重新实现被测逻辑的副本（见维度 6 第 1 条） | 低 | 测试有效性（详见 6-1） |
| 5-5 | src/app.rs（query_log）、src/dns_mw_audit.rs（blocked_counter） | query_log / blocked 统计链路无单测 | 低 | 统计功能回归 |

### 维度 6：测试严谨性（4 项）

| # | 位置 | 问题描述 | 严重程度 | 影响范围 |
| --- | --- | --- | --- | --- |
| 6-1 | tests/stats_test.rs:4-52、tests/cache_prefetch_test.rs:5-21 | **影子测试**：文件内自定义 StatsCounter 与 src/app.rs:320-326 的真实 Stats 字段逐行同构，但互不引用。真实实现改坏（如 avg 除零、单位换算错）这些测试照常绿。断言本身充分（含并发 1000 线程压测、浮点误差 < 0.01），但保护对象错了 | **高** | 统计逻辑实际无回归保护 |
| 6-2 | src/dns_conf.rs:2299 vs CHANGELOG.md:25 | 真实订阅测试断言 `rule_count >= 900`（合理容忍上游变化）；但 CHANGELOG「969 条规则加载」的精确 E2E 数字未沉淀为可重复执行的入库测试 | 中 | 变更声明可验证性 |
| 6-3 | src/dns_conf.rs:2232-2233 | test_domain_set 有两行被注释掉的断言（is_match 通配匹配），未说明跳过原因——「注释掉的红测试」比缺失更危险 | 低 | 通配匹配路径覆盖存疑 |
| 6-4 | src/config/parser/domain_set.rs:150-288 | parser 测试全部只测 happy path + 精确相等断言（好的一面），但无失败路径测试（缺 -name、缺 -file、未知 -type 的报错形态） | 低 | 解析错误提示质量 |

### 维度 7：工程规范（6 项）

| # | 位置 | 问题描述 | 严重程度 | 影响范围 |
| --- | --- | --- | --- | --- |
| 7-1 | webui/out/（40 个入库产物文件）、.gitignore:64-70 | webui/out 入库决策有依据（RustEmbed 编译期需要该目录，入库保证 clone 即可编译，CI 构建后覆盖）。代价：a) 前端改动产生带哈希文件名的大量 diff 噪声；b) 构建非确定性（Next chunk 哈希），易生无意义冲突；c) 曾因此出过事故（507455d6 修「静态产物入库损坏（TSD 污染）」）；d) **无产物与源码同步性校验**——忘记重新 build 就 cargo build 会嵌入旧 UI | 中 | 仓库卫生、构建一致性 |
| 7-2 | webui/package.json、.github/workflows/web-ui.yml | webui 无 lint/typecheck 门禁：无 eslint 配置、CI 只 build，TypeScript 错误只在 next build 顺带检查 | 中 | 前端代码质量 |
| 7-3 | .github/workflows/web-ui.yml:71-75、test.yml | web-ui.yml 的 paths 触发含 `src/**`（纯 Rust 改动也触发前端构建 job）；Test workflow 不跑 webui 相关检查，adblock 后端逻辑变更不触发前端验证 | 中 | CI 效率与覆盖联动 |
| 7-4 | 仓库根目录 | 杂物：PERFORMANCE_REVIEW.html、CACHE_HITRATE_AUDIT_V3.html、replay.sh、vibe-coding.sh、logs/smartdns-test.cache 残留；**localhost.key 私钥入库**（gitignore 中有意注释放行，dev 自签证书风险低，但 .key 入库非好实践）。**审计更正（2026-11-12）**：swagger-ui.html（openapi.rs `include_str!`）与 RESOLVE_EXAMPLES.txt（resolver.rs `after_help`）为编译期依赖，不是杂物 | 低 | 仓库卫生、安全习惯 |
| 7-5 | docs/ | 内部过程文档（两份历史 AUDIT-REPORT、EVAL 评估）与用户文档混放；任务描述中的 `doc/` 实际为 `docs/` | 低 | 文档组织 |
| 7-6 | Cargo.toml:44-45、src/updater.rs:64-65 | homepage/repository 仍指向上游 mokeyish/smartdns-rs，未反映 fork 身份；self-update 也指向上游 repo——fork 用户 `smartdns update` 会装到**上游版本** | 低（当前）/ 高（发布后） | fork 用户升级安全 |

---

## 三、改进建议与优先级排序

### P0 —— 用户可感知的正确性问题（建议立即处理）

**P0-1 修复 update API 静默吞错 + interval 语义错位**（对应 4-1、4-2）
- 做法：`src/api/domain_set.rs` update handler 中，`input.url` 存在时 `Url::parse` 失败应返回 400 错误而非 `.ok()` 静默 fallback；为 ApiError 增加 `BadRequest(String)` 变体，名称重复 / 未知 content_type / 非法 URL 统一走 400；interval 采用 `Option<Option<usize>>` 或显式 flag 区分「未传」与「清除」。
- 文件清单：`src/api/mod.rs`（ApiError）、`src/api/domain_set.rs`。
- 验证方式：为 update handler 补临时目录文件 IO 测试（非法 URL → 4xx；清 interval → 配置行中 -interval 消失）；curl 手工回归。

**P0-2 消除订阅双次下载/双次解析**（对应 3-1）
- 做法：`IDomainSetProvider` 增加 `get_domain_sets(&self) -> Result<(HashSet<WildcardName>, HashSet<WildcardName>)>`（block, allow 二元组），单次读取/下载后同时产出两个集合；保留旧方法或直接迁移调用方。
- 文件清单：`src/config/domain_set.rs`、`src/dns_conf.rs:1019-1043`（调用方）。
- 验证方式：现有 `test_adblock_domain_set` 端到端用例 + 新增「同一 provider 只发起一次 HTTP 请求」的计数测试（可用本地文件 provider 验证读文件次数）。

### P1 —— 文档补齐（新功能可发现性）

**P1-1 README / example.conf 补 adblock 与 domain-set 文档**（对应 1-1、1-2、2-1、2-2）
- 做法：README.md 与 README_zh-CN.md 增补「广告拦截」小节：`domain-set -type adblock` 配置示例、支持的语法子集表（`||domain^` / `@@||domain^` / hosts 行 / 跳过项）、与 `address /domain-set:名/#` 的配合关系、WebUI 截图；example.conf 在 domain-set 段落加 adblock 两行示例；AdblockTab 空状态文案改为说明自动联动行为。
- 文件清单：`README.md`、`README_zh-CN.md`、`etc/smartdns/example.conf`、`webui/src/app/dashboard/tabs/AdblockTab.tsx:243-244`。
- 验证方式：按文档示例配置启动服务，dig 验证拦截与例外放行。

**P1-2 补 webui 构建说明**（对应 1-5、2-4）
- 做法：CONTRIBUTING.md 增补「前端开发」一节（Node 20+、npm ci、npm run dev 的 rewrite 代理、npm run build 与 RustEmbed 关系）；AGENTS.md 目录结构补 webui/。
- 文件清单：`CONTRIBUTING.md`、`AGENTS.md`。
- 验证方式：新环境按文档从零构建一次。

### P2 —— 测试补强

**P2-1 为 src/api/domain_set.rs 补文件 IO 测试**（对应 5-1）
- 做法：create/update/delete 各补至少 2 用例（happy path + 冲突/不存在错误路径），用 tempfile crate 构造 managed_dir，断言 domain-set.conf 与 address.conf 的联动写入内容；不必构造完整 ServeState，可把 `find_provider_idx`/`write_provider_only`/`link_address_rule` 三个纯函数抽出直接测。
- 文件清单：`src/api/domain_set.rs`（内联 `#[cfg(test)]` 模块）、`Cargo.toml`（dev-dependencies 加 tempfile，若未有）。
- 验证方式：`cargo test domain_set`。

**P2-2 把影子测试接到真实代码**（对应 6-1、5-4）
- 做法：删除 tests/stats_test.rs、tests/cache_prefetch_test.rs 中的本地副本结构，改为 `use smartdns::...` 引用真实类型（需要将 app.rs 的 Stats 与 dns_mw_cache 的 PrefetchConfig 等以 pub 导出，或在 src 内以 `#[cfg(test)]` 模块迁移这些用例）。
- 文件清单：`tests/stats_test.rs`、`tests/cache_prefetch_test.rs`、`src/app.rs`、`src/lib.rs`（导出）。
- 验证方式：迁移后 `cargo test --test stats_test --test cache_prefetch_test` 全绿；人为改坏 `avg_query_time_ms` 除零逻辑确认测试会红。

**P2-3 补 adblock 解析边界用例**（对应 5-2、6-4）
- 做法：`src/config/domain_set.rs` 测试模块补：CRLF 行尾、`@@|http://` 例外 URL 行、大小写混合域名、`domain.com$dnstype` 修饰边界、parser 失败路径（缺 -name / 未知 -type）。
- 文件清单：`src/config/domain_set.rs`、`src/config/parser/domain_set.rs`。
- 验证方式：`cargo test adblock`。

### P3 —— 工程规范与前端质量

**P3-1 webui/out 同步性校验**（对应 7-1）
- 做法（二选一）：a) web-ui.yml 的 webui job 构建后增加 `git diff --exit-code webui/out`（仅校验，不阻塞主构建可先 warn）；b) build.rs 检测 webui/src 比 webui/out 新时打印警告。根治方案是 CI 中构建并注入产物（现已做）+ 移除入库（需权衡 clone 即编译的便利）。
- 文件清单：`.github/workflows/web-ui.yml`。
- 验证方式：改动 webui/src 后不重新 build 提交，CI 应报差异。

**P3-2 webui 补 lint/typecheck 门禁**（对应 7-2、5-3）
- 做法：补 eslint 配置（next lint 需要显式配置文件），package.json 增加 `typecheck: tsc --noEmit` 脚本，web-ui.yml build 前先跑两者。
- 文件清单：`webui/eslint.config.mjs`（新建）、`webui/package.json`、`webui/tsconfig.json`、`.github/workflows/web-ui.yml`。
- 验证方式：CI 中 lint/typecheck 步骤通过；故意引入类型错误验证会红。

**P3-3 前端代码清理**（对应 4-3、4-4、4-5、4-9、4-10）
- 做法：补 `apiPut` 封装替换内联 fetch；半透明色改 `alpha()` 或模板字符串统一写法；EditSubscriptionDialog 改 useEffect 初始化或 `key={target.name}` 重挂载；统一 SubscriptionInput 字段名；预置 URL 提取为常量/配置；统计卡片与 page.tsx MetricCard 合并复用。
- 文件清单：`webui/src/lib/api.ts`、`webui/src/app/dashboard/tabs/AdblockTab.tsx`、`webui/src/app/dashboard/page.tsx`。
- 验证方式：`npm run lint && npm run typecheck && npm run build`；手工冒烟增删改订阅。

**P3-4 仓库卫生**（对应 7-4、7-5、7-6、1-4、4-6）
- 做法：删除根目录过程产物（两个 html、replay.sh、vibe-coding.sh、RESOLVE_EXAMPLES.txt 视归属决定）、logs/smartdns-test.cache；localhost.key 从 git 移除（保留本地）；修正 "DoaminSet" 拼写；确认 Cargo.toml repository 与 updater 指向 fork 还是上游（发布前必须决策）；docs/ 区分用户文档与内部过程文档（子目录或命名约定）。
- 文件清单：根目录若干、`src/dns_conf.rs`、`Cargo.toml`、`src/updater.rs`。
- 验证方式：`git ls-files` 复查；`cargo build` 与 `smartdns update` 行为验证。

### P4 —— 架构演进（可延后）

**P4-1 interval 功能落地或移除**（对应 3-4）
- 做法：二选一——a) 落地：dns_mw_cache 或独立后台任务按 interval 定期重新拉取 Http provider 并 reload；b) 移除：WebUI 隐藏该输入框 + 配置文档标注「保留字段，暂不生效」。在落地前，至少先做 b) 止损。
- 文件清单：`webui/src/app/dashboard/tabs/AdblockTab.tsx`（短期）、`src/app.rs` 或新模块（长期）。
- 验证方式：落地后用本地 mock HTTP 服务验证定时刷新；短期方案验证 UI 不再展示。

**P4-2 dns_conf.rs 拆分与 parser 去重**（对应 3-3、4-8）
- 做法：dns_conf.rs 的 RuntimeConfig 装配段（含 domain_set 加载统计）拆到独立模块；File/Http provider parser 抽公共 options 解析循环；link_address_rule 提取公共 retain+write。
- 文件清单：`src/dns_conf.rs`、`src/config/parser/domain_set.rs`、`src/api/domain_set.rs`。
- 验证方式：纯重构，`cargo test` 全绿 + `just cleanliness`。

---

## 附录：问题统计

| 维度 | 高 | 中 | 低 | 小计 |
| --- | ---: | ---: | ---: | ---: |
| 1 文档一致性 | 1 | 2 | 2 | 5 |
| 2 文档缺漏 | 1 | 2 | 1 | 4 |
| 3 架构优化点 | 0 | 3 | 2 | 5 |
| 4 代码写法 | 1 | 4 | 5 | 10 |
| 5 测试覆盖率 | 1 | 2 | 2 | 5 |
| 6 测试严谨性 | 1 | 1 | 2 | 4 |
| 7 工程规范 | 0 | 3 | 3 | 6 |
| **合计** | **5** | **17** | **17** | **39** |

---

## 五、增量审计（2026-11-12，基线 `507455d6` → `8f0e003f`）

远端新增 4 提交：`756002da`（审查修复 TCP 背压/拦截统计/邻居缓存/订阅 API 安全）、
`759a3144`（conf_dir 推导修复）、`db45d5a0`（订阅保存卡死与日志丢失修复）、
`8f0e003f`（全量配置设置页 + routes! 按路径拆分）。本节核对上述提交对原 39 项问题
的影响，并登记新增代码的审计发现。

### 5.1 原有问题状态变化

| 原编号 | 原状态 | 新状态 | 依据 |
| --- | --- | --- | --- |
| 4-1（update 静默吞 URL 错） | 高危未修 | **已修复（2026-11-12）**：update 路径与 create 共用 `parse_subscription_url`（Url::parse 失败显式 400 + scheme 限定 http/https），静默 fallback 已消除 | 本轮 P0-1 实施 |
| 4-1（interval 无法清除） | 高危未修 | **已修复（2026-11-12）**：`UpdateDomainSet.interval` 改 `Option<Option<usize>>`（serde default + deserialize_with 区分「未传」与「传 null」），未传=保留、null=清除、数字=设新值 | 本轮 P0-1 实施 |
| 4-2（500 代替 400） | 中危未修 | **已修复（2026-11-12）**：ApiError 新增 `BadRequest(String)`（400）；domain_set/config 两模块全部客户端输入错误（非法 URL、名称重复、未知 content_type、unknown family/key、缺 index、行非配置行等）统一改 400 | 本轮 P0-1 实施 |
| 3-2（routes! workaround） | 中危 | **已根治**：根因查明（fork 宏折叠语义），全部模块按路径拆分，AGENTS.md 补记规约（8f0e003f）。复查 src/api 全部 14 个 routes()：serve_dns.rs 的 GET+POST 同路径合并属规约允许写法，均合规 | 逐文件 grep 核查 |
| 5-1（src/api 零测试） | 高危 | **部分缓解**：src/api/config.rs 新增 9 个单元测试（split/apply/validate/family 全覆盖纯函数）；**domain_set.rs 已补 10 个单元测试（2026-11-12）**：URL/scheme/名称/路径校验 + 双层 interval 语义 + 配置 round-trip + address 联动行 | cargo test（本轮新增 10） |
| 6-1（影子测试） | 高危 | **未修**：tests/stats_test.rs 仍是同构副本结构 | 文件核对 |
| 7-3（CI 覦盖联动）、7-2（webui 无 lint） | 中危 | 未修 | — |
| 7-4（仓库杂物/localhost.key） | 低危 | **已修复（2026-11-12）**：删除 PERFORMANCE_REVIEW.html、CACHE_HITRATE_AUDIT_V3.html、replay.sh、vibe-coding.sh、localhost.key（本地保留，gitignore 恢复忽略），清理 logs/smartdns-test.cache。**审计更正：swagger-ui.html 与 RESOLVE_EXAMPLES.txt 为 include_str! 编译期依赖，保留** | git rm + checkout 恢复验证 |
| 1-1（README 无 adblock） | 高危 | 未修（README 只补了设置页描述，adblock/domain-set 仍零提及） | README 核对 |
| N-6（value 含 # round-trip 破坏） | 中危 | **已修复（2026-11-12）**：`validate_conf_line` 增加 round-trip 检查，重建行经 split_conf_line 回读值必须一致；nftset 无空白 `#` 不受影响 | 本轮实施 + 回归测试 |

### 5.2 新增代码审计发现（8f0e003f 的 config.rs / config-catalog.ts 等）

| # | 位置 | 问题描述 | 严重程度 |
| --- | --- | --- | --- |
| N-1 | src/api/config.rs:272-332 `apply_scalar_updates` | **新增/追加路径不感知 group 边界**：`updates` 中的键若在 group 块内出现过（被正确跳过）、在全局从未出现，会在文件末尾追加全局行——行为正确；但若该键**只在 group 内有值**，追加后全局生效会改变实际语义（原 group 内行为不变，新增全局行对全部查询生效）。低概率场景（同键既在 group 内又在 updates 中），记录备查 | 低 |
| N-2 | src/api/config.rs:328-330 | `apply_scalar_updates` 尾部换行处理：`text.ends_with('\n') \|\| !updates.is_empty()` ——空 updates 且原文本无尾换行时不补；但**有 updates 且原文本无尾换行**时会补一个。行为无害但条件语义混杂（updates 非空必然重写，此时统一补尾换行合理），仅记录 | 低 |
| N-3 | src/api/config.rs:539-577 post_lines Update | **跨家族 key 切换已守卫**（index 行必须仍属本家族）；但 update 把行替换为新 key 后**不校验新行与文件中其它同 key 行的组合语义**（如两条 `bind [::]:53` 重复监听，启动 reload 会失败）。reload 失败时配置已落盘——原子写保证了不截断，但**未做「校验通过才 reload、reload 失败回滚文件」**，配置文件与运行态可能出现静默漂移（reload 失败仅日志，文件保持新值）。与 domain_set.rs 的 update 同模式，属系统性取舍：**写盘成功 + reload 失败 → 磁盘与运行态不一致**，需要用户二次手动 reload。建议后续统一加「reload 失败回滚或返回 5xx 警示」 | 中 |
| N-4 | src/api/config.rs:63-141 SCALAR_KEYS vs webui config-catalog.ts | **已对账**：77 键逐一 diff 完全一致（含别名归一化）。此项通过，无问题 | — |
| N-5 | webui/src/lib/config-catalog.ts | helper 函数（`text()/num()/bool()`）与对象字面量（select 项用 `key:` 字段）**两种写法混用**，642 行内 75 项中 70 项走 helper、5 项 select 走字面量。补一个 `select()` helper 可统一 | 低 |
| N-6 | src/api/config.rs:236 split_conf_line | 尾注释判定 `rest.find(" #")` 或 `rest.find("\\t#")`：`" #"`（空格+井号）已覆盖 `\\t#` 前有 `\\t` 的场景吗？——`find(" #")` 匹配**任意位置**第一个 `" #"`，若值本身含 ` #`（如 `server-name # 主机名`）会被误判为注释分界。但 put_options 已拦截值含 `#`（:408-411），add/update lines 未拦 value 含 `#`——**post_lines 的 value 含 `" #"` 时会被 split 拆成值+注释，回读时注释丢失进 value**（round-trip 破坏）。触发条件：用户在行编辑器里输入含 ` #` 的值 | 中 |
| N-7 | src/api/config.rs:386-387 get_options | `conf_file: Some(file.to_string_lossy()...)` 恒为 Some（read_conf_file 失败已提前返错）——字段建模为 Option 但实际恒 Some，轻微建模冗余 | 低 |
| N-8 | webui/src/lib/api.ts useMutateConfigLines / SettingsTab | 未审查到测试（webui 整体 0 测试延续，属原 5-3 范围）；SettingsTab.tsx 430 行含 77 项渲染与批量保存逻辑，属前端最大单文件，后续拆分候选 | 低 |

### 5.3 增量审计结论

- **8f0e003f 整体质量高**：真实 parse_config 校验、原子写、串行锁、陈旧索引守卫、
  77 键对账、9 个纯函数单测——新代码本身没引入高危问题。
- **P0-1 的两个核心点（update 静默吞 URL 错 + interval 无法清除）已随本轮实施修复**
  （2026-11-12，见 5.1 表状态更新）；N-6 同轮修复。N-3（reload 失败不回滚）
  仍开放，属系统性取舍，建议单独排期。
- 测试基线：**411 通过 / 0 失败 / 13 ignored**（较上次 399+13 净增 12，与 config.rs
  新增 9 测试 + dnsmasq/dns_mw_addr 新增 3 测试吻合）。本轮 P0-1/N-6 再增
  10 + 0 个测试（domain_set.rs 10 个，config.rs 扩充 3 断言至原用例内）。
