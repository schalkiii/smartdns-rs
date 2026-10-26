# 测试覆盖率矩阵 — 2026-10-26

生成方式（可复现）：遍历 `src/**/*.rs`，统计 `#[test]` / `#[tokio::test]` 数量与行数（见文末命令）。
当前基线：`src` 167 文件 / 33 754 行 / **370 个单元测试（正则计数，含 feature 门控模块）**；`tests/` 3 文件 18 个集成用例；
`cargo test --features=disable_icmp_ping` = **351 + 10 + 7** 全绿。

## 1. 按目录汇总

| 目录 | 文件 | 行数 | 测试 | 零测试文件 |
| :--- | ---: | ---: | ---: | ---: |
| `src/`（根，核心中间件/客户端/配置） | 34 | 17 010 | 198 | 8 |
| `src/api`（Web UI + HTTP DNS） | 15 | 1 361 | **0** | **15** |
| `src/server`（UDP/TCP/TLS/HTTPS/QUIC/H3 监听） | 9 | 1 311 | **0** | **9** |
| `src/config`（schema + 规则组） | 15 | 2 574 | 32 | 7 |
| `src/config/parser`（配置解析） | 37 | 4 605 | 73 | 1 |
| `src/config/parser/nom_recipes` | 5 | 211 | 4 | 2 |
| `src/infra`（ping/dhcp/内存映射/进程） | 12 | 2 580 | 32 | 3 |
| `src/libdns`（hickory 兼容层） | 2 | 162 | 0 | 2 |
| `src/libdns/custom`（连接池/预热） | 4 | 1 524 | 17 | 3 |
| `src/zone` + `zone/providers` | 7 | 531 | 2 | 6 |
| `src/service/*`（各平台服务管理） | 14 | 1 471 | 8 | 11 |
| `src/ffi`（nftset，Linux 专用） | 3 | 115 | 0 | 3 |
| `src/third_ext` | 8 | 299 | 4 | 5 |
| **合计** | **167** | **33 754** | **370** | **77** |

## 2. 关键缺口（按风险排序）

| # | 缺口 | 量化 | 为什么是风险 | 补测前置条件 |
| :--- | :--- | :--- | :--- | :--- |
| G1 | `src/api/*` 全部无测试 | 15 文件 / 1 361 行 / 0 用例，含 4 个写接口（清缓存、重载配置、address 增删改） | 接口语义/序列化/错误码变化无回归网 | 需要能构造 `ServeState`（依赖 `DnsClientHandle`）；建议先抽一个只依赖 `Arc<RuntimeConfig>` 的路由层 |
| G2 | `src/server/*` 监听器无测试 | 9 文件 / 1 311 行 / 0 用例 | bind 失败、握手超时、缓冲区设置等路径只能人工联调 | 可用临时端口起监听 + 发送真实报文；TLS 需自签证书夹具（`rustls_self_signed` 已有生成器） |
| G3 | 并发门只有纯判定覆盖 | `should_refuse_query` 4 例；真实链路（含 `active_queries` 归还）未测 | 归还逻辑写错会永久降级并发 | 需要完整 `App` 装配；或在 dispatch 层抽成不依赖 socket 的 `run_once(requests) -> 响应列表` |
| G4 | 双栈优选无法注入延迟 | 判据 `is_that_faster` 4 例；`which_faster`/`multi_mode_ping_fastest` 依赖真实 ICMP | 优选策略回归只能线上观察 | 把 `ping_fastest` 调用改成注入式（`fn` 指针或 trait），属小幅架构改动 |
| G5 | 跨中间件链路无端到端用例 | `tests/` 仅 18 例，且无「cname 改写 → 下游规则命中」这类组合 | 链序/上下文传递错误难发现 | 见 §4 的阻塞记录：`DnsMockMiddleware` 与 cname 改写的 continuation 交互未定位 |
| G6 | `src/zone` / `zone/providers` 基本无测试 | 7 文件 / 531 行 / 2 用例 | 静态记录、local-ptr、hosts 展开的边界未覆盖 | 可直接用 `ZoneProvider` 纯函数测，无需网络 |
| G7 | `src/ffi`（nftset）无测试 | 3 文件 / 115 行 / 0 用例，且只在 Linux+nft 编译 | Windows 本地检不到（见 AGENTS.md 的 Linux 门控段） | 只能在 Linux CI 或临时去门控验证 |

## 3. 本轮新增用例

| 用例 | 覆盖点 | 结果 |
| :--- | :--- | :--- |
| `app::tests::test_should_refuse_query` | `max-query-limit`：0=不限、等于上限放行、严格超过拒绝、后台豁免 | PASS |
| `dns_mw_dualstack::tests::test_is_that_faster` | 切换判据：差值须严格大于阈值、本族更快不切换 | PASS |
| `dns_mw::tests::test_bogus_nxdomain_filters_mocked_ip` | `bogus-nxdomain` 命中→整条应答转 NXDOMAIN；未命中→正常返回 | PASS |
| `dns_mw::tests::test_ipnet_from_subnet_*`（3 例） | ECS 前缀校验：合法 v4/v6、非法 33/129、IPv4-mapped | PASS |
| `dns_mw_addr::tests::test_rr_ttl_bounds_folds_fixed_ttl` | `rr-ttl`/min/max 折叠为边界；`reply-max` 不参与 | PASS |
| `dns_mw_addr::tests::test_clamp_ttl_bounds` | 夹取顺序 min→max | PASS |

## 4. 已知阻塞（附证据，避免重复踩）

1. **cname 改写后的 mock 链路**：用例「cname 规则 → address 规则」始终返回
   `NoRecordsFound(target.example.com)`；同一链上**直查 target 能命中 mock**（已用断言确认），
   说明 rewrite 之后的 `next.run` 没有落到 mock 上。需要先给 `DnsMiddlewareHost` 加链路追踪
   （记录每层 middleware 名）再定位，不能靠猜。
2. **`DnsMockMiddleware` 的匹配键**：以 `query().original()` 取值；`with_cname` 会整体替换查询，
   因此理论上 original 也变成新名——与上一条的表现矛盾，同样需要追踪先定位。
3. **`ServeState` 构造成本**：`src/api` 的路由测试需要一个可用的 `DnsClientHandle`；
   建议先做「把 handler 依赖收窄成 trait」的小重构，再补测试。

## 5. 复现命令

```bash
# 逐目录统计（Git Bash / WSL）
python - <<'PY'
import os, re, collections
rows = []
for root, _, files in os.walk('src'):
    for f in files:
        if not f.endswith('.rs'):
            continue
        p = os.path.join(root, f).replace(os.sep, '/')
        t = open(p, encoding='utf-8').read()
        rows.append((p, len(t.splitlines()), len(re.findall(r'#\[(?:tokio::)?test', t))))
agg = collections.defaultdict(lambda: [0, 0, 0, 0])
for p, loc, tests in rows:
    a = agg[os.path.dirname(p)]
    a[0] += 1; a[1] += loc; a[2] += tests; a[3] += (tests == 0)
for d in sorted(agg):
    print(d, agg[d])
print('TOTAL', len(rows), sum(r[1] for r in rows), sum(r[2] for r in rows),
      sum(1 for r in rows if r[2] == 0))
PY

# 只跑单元测试 / 集成测试
cargo test --features=disable_icmp_ping --bin smartdns
cargo test --features=disable_icmp_ping --tests
```
