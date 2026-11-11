# smartdns-rs 支持 adblock 格式订阅：可行性评估

> 评估目标：类似 AdGuard Home，支持直接订阅 adblock/AGH 格式规则列表（`||domain^`、`@@||domain^`、`##cosmetic`、hosts 格式等），用于 DNS 层拦截。
> 日期：2026-11-11

## 一、结论（先行）

**可行，难度低-中（约 1-2 天工作量），建议做。**

核心原因：smartdns-rs 的规则架构与 adblock 格式天然契合——

1. **基础设施全部就绪**：`domain-set -type list -url` 已支持 HTTP 订阅拉取（ureq 客户端、gzip 解压）、`WildcardName` 已支持 `*.`/`+.example.com` 子域通配语义（与 adblock `||domain^` 等价）、`address` 规则已支持 `#`（SOA 阻断）和 `-`（IGNORE 返回空）。
2. **adblock 格式对 DNS 场景是个小子集**：AdGuard Home 数百条语法里，DNS 层只需要处理 3-4 种模式（见下）；其余（CSS 选择器 `##`、scriptlet `$script` 等）属浏览器层过滤，DNS 层直接忽略即可——AdGuard Home 对 DNS 查询也只走 hostrule/networkrule 的域名匹配子集。
3. **改动集中在一个解析函数**：新增一个 `adblock` content-type，把 `read_to_domain_set` 换成 adblock 感知版本。

## 二、adblock 格式与 DNS 层的映射关系

| adblock 语法 | 含义 | smartdns-rs 映射 | 处理 |
|---|---|---|---|
| `||example.com^` | 拦截 example.com 及全部子域 | `+.example.com`（WildcardName） | **支持** |
| `@@||example.com^` | 例外白名单（覆盖拦截规则） | 优先级高于拦截规则的白名单 | **支持**（需新增例外集） |
| `|http://example.com` | 精确 URL 前缀 | 不适用（DNS 层无 URL） | 忽略 |
| `example.com`（裸域名行） | 拦截该域名及子域的请求 | `+.example.com` 或精确匹配 | **支持** |
| `##.ad-banner`、`example.com##...` | CSS/HTML 元素隐藏 | 浏览器层 | 忽略 |
| `$script`、`$image`、`$third-party` 等修饰 | 内容类型/来源修饰 | DNS 层无细分 | 忽略修饰符，按域名处理（AGH 同样做法） |
| `0.0.0.0 example.com` / `127.0.0.1 example.com`（hosts 格式） | hosts 拦截 | 精确域名 + 子域通配 | **支持**（AGH 混合列表常见） |
| `!` 注释、`#` 注释 | 注释 | — | 跳过 |
| `/regex/`、`*` 通配符中缀 | 正则/复杂通配 | 正则可映射但成本高 | 首期忽略 |

AdGuard Home 的 DNS 拦截本质也是：解析 adblock 语法 → 抽取域名模式 → 匹配即返回 0.0.0.0/空。smartdns-rs 已有等价输出能力。

## 三、实现方法（推荐方案）

### 方案：新增 `-type adblock` content-type（约 300-400 行）

**1. 解析层（config/parser/domain_set.rs + config/domain_set.rs）**

```text
domain-set -type adblock -name my-adblock -url https://example.org/easylist.txt
```

- `DomainSetContentType` 增加 `Adblock` 变体（当前只有 `List`，parser 是单行 `alt`，加一个 `tag_no_case("adblock")` 即可）。
- 新增 `read_adblock_to_domain_set(text) -> (HashSet<WildcardName>, HashSet<WildcardName>)`：
  - 逐行解析，产出**拦截集** + **例外集**两个集合；
  - `||domain^` → `+.domain`（拦截集）；`@@||domain^` → `+.domain`（例外集）；
  - hosts 行（`IP domain`）→ domain（拦截集）；
  - 含 `##`、`$`、`/`（正则）、`|http` 的行跳过；
  - 例外集查询在拦截集之前，命中例外则放行。

**2. 数据结构（config/domain_set.rs）**

- `DomainSet`（实际是 `HashSet<WildcardName>`）需要携带例外集，或建一个平行结构 `AdblockSet { block: DomainSet, allow: DomainSet }`。
- `DomainRule` 的 address 动作由用户的 `address /domain-set:adblock/` 规则指定（`#` SOA 或 `-`），复用现有 `DomainRuleMap`，无需新增动作。

**3. 运行时（dns_conf.rs）**

- 启动时拉取与现在一致（一次性）；如需订阅自动更新，把已解析但未使用的 `interval` 字段接到一个后台 tokio 任务（该字段解析了但运行时未用，属顺手补全）。

**4. 匹配路径（零改动或极小改动）**

- `DomainMap::find` 查询 `+.domain` 走现有 wildcards 索引，性能与手写 `*.domain` 规则完全一致。

### 用户配置示例（实现后）

```ini
# 订阅 easylist（adblock 格式）
domain-set -type adblock -name ads -url https://easylist-downloads.adblockplus.org/easylist.txt

# 用 SOA 空应答拦截（返回 NXDOMAIN 语义）
address /domain-set:ads/# -

# 或返回 0.0.0.0（AGH 风格）
address /domain-set:ads/0.0.0.0
```

## 四、难易度分解与工作量估计

| 步骤 | 内容 | 难度 | 估计 |
|---|---|---|---|
| 1 | `DomainSetContentType::Adblock` parser | 极低 | 30 min |
| 2 | adblock 行级解析函数（含 `||`/`@@`/hosts/跳过规则）+ 单测 | 低 | 2-3 h |
| 3 | 例外集穿透拦截集的查询逻辑（domain rule 匹配处） | 中 | 2-3 h |
| 4 | 真实订阅回归测试（easylist ~50k 行）+ 边界（空行/超大行/CRLF/BOM） | 低 | 1-2 h |
| 5 | 文档 + example.conf 示例 | 低 | 1 h |

**风险点**（均可控）：
- 例外规则 `@@` 的优先级语义：AGH 中例外优先于拦截。实现为例外集先查即可，但需在文档说明与 AGH 语义一致。
- 部分列表混合多语法，解析覆盖率不必 100%，跳过不认识的行是 AGH 亦然的行为。
- 5 万行级列表的启动解析耗时：逐行字符串切分 O(n)，~10ms 级，可忽略。

## 五、备选方案对比

| 方案 | 描述 | 优劣 |
|---|---|---|
| **A. 内置 adblock type（推荐）** | 如上 | 一次开发，用户零转换成本；与 AGH 体验对齐 |
| B. 转换脚本 | 提供 `adblock→list` 离线转换工具（如 `sed`/脚本） | 零代码风险，但用户需自行定期转换，订阅更新体验差 |
| C. 内嵌 adblock-rust crate | 用 Brave 的 adblock-rust 引擎解析 | 功能最全（含 $modifier/regex），但依赖重（+几十个 crate）、面向浏览器场景，DNS 场景杀鸡用牛刀 |

**推荐 A**：实现简单、依赖零增加、覆盖 DNS 层所需的全部语法。

## 六、总结

- **可行性**：✅ 高。基础设施（HTTP 订阅、通配匹配、拦截动作）全部已就绪，只差一个格式解析层。
- **难易度**：低-中，1-2 天含测试文档。
- **实现方法**：新增 `-type adblock` content-type + 拦截/例外双集合解析 + 例外优先查询；用户侧仅需两行配置即可订阅 easylist 类列表。

---
*评估基于 smartdns-rs 当前 HEAD（含 2026-11-11 上游合并与性能优化）。*
