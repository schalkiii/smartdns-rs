# Rust 移植版 vs C 原版：功能缺口评审

评审对象：`pymumu/smartdns`（C，`dfc9ef4`）vs 本仓库 `smartdns-rs`（基线 v0.13.4 + 当前 Unreleased）。
检索方式：XERJ 本地索引（`--prefix smartdns-c` / `--prefix sdrs`）+ 逐文件读源码。

复现：

```shell
xerj --insecure --port 9300 --data-dir ~/xerj-data &
xerj autoindex /path/to/smartdns   --prefix smartdns-c --url http://127.0.0.1:9300
xerj autoindex /path/to/smartdns-rs --prefix sdrs       --url http://127.0.0.1:9300
xerj search --url http://127.0.0.1:9300 --prefix smartdns-c "<问题>"
```

## 1. 配置指令覆盖

C 版配置表（`src/dns_conf/dns_conf.c` 的 `CONF_*` 宏）共 114 个指令；Rust 解析器
（`src/config/parser/`）支持 127 个键。逐项比对后，**C 有而 Rust 无的共 37 个（已补齐 16 个；group-end 为提取误报，实际已支持；剩余 20 个）**：

| 分类 | 指令 | 影响 | 状态 |
| :--- | :--- | :--- | :--- |
| 静态记录 | `txt-record` | 自定义 TXT 记录 | **本次已实现** |
| 静态记录 | `srv-record`（增强） | C 版支持多组、Rust 单值 | **本次已实现（多值累积）** |
| 缓存 | `cache-mem-size` | 缓存内存上限 | **本次已实现（估算占用 + 超限 LRU 淘汰）** |
| 缓存 | `serve-expired-prefetch-time` | 过期前预取窗口，`src/dns_server/cache.c:87` | **本次已实现** |
| 并发/网络 | `max-query-limit` | 并发查询上限，超出拒绝，`src/dns_server/dns_server.c:483` | **本次已实现** |
| 并发/网络 | `socket-buff-size` | socket 收发缓冲区 | 待实现 |
| 行为 | `force-no-CNAME` | 应答中去掉 CNAME 只留地址，`src/dns_server/address.c:195`、`answer.c:64` | **本次已实现** |
| 行为 | `local-domain` | 本地域后缀走 mDNS 组 + hosts 条目后缀展开 | **本次已实现** |
| PTR | `local-ptr-enable` | 用 hosts 回答本机地址的 PTR（Rust 已有 `LocalPtrZoneProvider`，缺开关） | **本次已实现** |
| 防火墙 | `ipset` / `ipset-timeout` / `ipset-no-speed` | ipset 下发与超时 | 待实现（nft 已有部分能力） |
| 防火墙 | `nftset-timeout` / `nftset-debug` / `nftset-no-speed` | nftset 超时/调试 | `timeout`/`debug` **已实现**；`no-speed` 待实现 |
| 日志 | `log-syslog` / `audit-syslog` / `audit-console` / `log-color` | 日志后端与着色 | 低优先级 |
| 日志 | `audit-SOA` / `audit-console` | SOA 应答也审计 / 审计同时打印到控制台 | **本次已实现** |
| 证书 | `bind-cert-generate` / `-root-key-file` / `-san` / `-validity-days` | 自签证书自动生成 | 低优先级（可用外部证书） |
| 运维 | `no-daemon` / `no-pidfile` / `restart-on-crash` | 守护进程行为 | 不移植（交给 systemd/Windows 服务） |
| 运维 | `data-dir` | 数据目录（相对 cache-file 按此解析） | **本次已实现** |
| 运维 | `debug-save-fail-packet*` | 失败报文落盘 | 低优先级 |
| ACL | `acl-enable` / `ip-rules` | 客户端/IP 访问控制 | 待评估 |
| 其它 | `odhcpd-lease-file` | OpenWrt odhcpd 租约解析 | **本次已实现** |
| 其它 | `ddns-domain` / `plugin` / `group-match` | DDNS、插件框架、组匹配 | 不移植 / 待评估 |

## 2. 本次实现：`txt-record`

- 语法与 C 版一致：`txt-record /example.com/text`；无 `/domain/` 前缀时落到根域。
- 同一域名的多行累积为同一个 TXT RRset（C 版 `_conf_domain_rule_txt_record` 追加链表）。
- 落点：`src/config/parser/txt.rs`、`DomainRule.txt`、`RuleGroup.txt_records`、
  `DomainRuleMap::create`、`ZoneProvider` 的 `RecordType::TXT` 分支。
- 测试：`config::parser::txt::tests::test_parse`、`dns_conf::tests::test_txt_record_accumulates_per_domain`。

## 3. 已核对的一致性项（未发现偏差）

- `cname` 规则链式解析（C `src/dns_server/cname.c`），Rust 在 `dns_mw_cname.rs` 逐级重写查询。
- `serve-expired` / `serve-expired-ttl` / `serve-expired-reply-ttl` 语义一致。
- `expand-ptr-from-address`、`local-ptr` zone provider 存在且行为对齐。
- `https-record` 的 `#`(SOA) / `-`(忽略) / `noipv4hint` 过滤与 C 版一致。

## 4. 建议的下一步顺序

1. `max-query-limit`（保护性配置，实现简单，收益直接）。
2. `force-no-CNAME`（应答裁剪，影响解析结果可预期性）。
3. `serve-expired-prefetch-time`（缓存命中率）。
4. `local-ptr-enable`（已有 provider，只差开关）。
5. `cache-mem-size`（需要逐条缓存内存核算，改动面大）。

## 5. 第二批实现（保护性 / 行为 / 缓存）

| 指令 | C 语义 | Rust 落点 |
| :--- | :--- | :--- |
| `max-query-limit` | 并发请求数超过上限时直接回 REFUSED，告警 120s 节流（`dns_server.c:483`） | `app.rs` 调度循环入口 + `server::refused_response` |
| `force-no-CNAME` | 应答不携带 CNAME，只留解析结果 | 新增最外层 `DnsForceNoCnameMiddleware`，`answers_mut().retain` 裁掉 CNAME（CNAME 查询本身不裁） |
| `local-ptr-enable` | 默认开启；关闭后本机地址 PTR 不再本地应答（`ptr.c:269`） | `DnsZoneMiddleware::with_local_ptr` 决定是否注册 `LocalPtrZoneProvider` |
| `serve-expired-prefetch-time` | 过期前预取窗口；未配置时按 `serve-expired-ttl/2` 且上限 8h 推导（`dns_cache.c:704`） | 命中即预取阈值改为读配置，未配置沿用现有 600s 默认 |

测试：`dns_conf::tests::test_parity_directives`（解析 + 默认值）、
`dns_mw_cname::tests::test_strip_cname_records`（CNAME 裁剪）。
`cargo test` 326 + 10 + 7 全绿，clippy 零告警，fmt 干净。

## 6. 剩余优先级（滚动更新，当前 20 项）

2. `ipset` / `ipset-timeout` / `ipset-no-speed`、`nftset-no-speed`（防火墙集成）
3. `acl-enable` / `ip-rules`（访问控制，需要先定配置语法）
4. `log-syslog` / `audit-syslog`（syslog 后端）
5. `bind-cert-generate` 等 4 项自签证书配置（需引入 rcgen）
6. `debug-save-fail-packet*`、`group-match`
7. 不移植：`plugin`、`ddns-domain`、`no-daemon` / `no-pidfile` / `restart-on-crash`

## 7. 第三批实现：SRV 多值

`srv-record` 同一域名多行原先后者覆盖前者；现改为累积成同一 SRV RRset
（`DomainRule.srv: Option<Vec<SRV>>`），由 `ZoneProvider` 一次返回多条，
与 C 版链表语义一致。测试：`dns_mw_zone::tests::test_srv_record_multiple`。

## 8. 第四批实现：nftset 超时/调试、socket 缓冲

- `nftset-timeout`：开启后 nftset 元素超时取答案 TTL * 3（C `context.c:672`），
  `nftset::add` 的 timeout 参数不再恒为 0；`nftset-debug` 打开逐条添加日志。
  顺带把 v4/v6 两段重复的下发循环收敛成 `add_addrs`。
- `socket-buff-size`：监听 socket 在 bind 前设置 SO_RCVBUF/SO_SNDBUF，
  `serve()` 把配置透传给 UDP/TCP/DoT/DoH/DoH3/DoQ 全部监听器。
- 测试：`dns_conf::tests::test_parity_directives` 覆盖两个指令的解析。

## 9. 第五批实现：server-http3、data-dir、日志颜色与审计开关

- `server-http3`：与 `server-h3` 等价的别名（解析为 `h3://`）。
- `data-dir`：相对 `cache-file` 按 data-dir 解析，绝对路径不变。
- `log-color`：默认开启，控制台日志 ANSI 着色可关闭。
- `audit-SOA`：默认关闭；SOA-only 应答不进审计（对应 C `audit.c:127`）。
- `audit-console`：审计记录同时打印到控制台（对应 C `audit.c:200`）。
测试：`test_parity_directives`、`test_data_dir_resolves_relative_cache_file`、
`test_config_nameserver`（server-http3）。

## 10. 第六批实现：local-domain + 测试稳定性

- `local-domain`：配置的后缀走 mDNS 服务组；hosts 查询在该后缀下剥离后缀查基数名，并把命中
  记录名改写为查询名（等价于 C 版给 hosts 条目展开后缀）。
- 测试稳定性：`dns_client::tests::test_with_default` 与 `dns_mw_ns::tests::test_edns_client_subnet`
  依赖真实外网，并行负载下会超时（此前 CI 偶发红的原因之一），已标记 `#[ignore = "requires Internet access"]`，
  需要时用 `cargo test -- --ignored` 单独跑。

## 11. 第七批实现：odhcpd-lease-file

- 租约存储在 dnsmasq 之外可再挂一个 odhcpd DHCPv6 租约文件，两者合并到同一份映射；
  任一文件 mtime 变化都会触发重载。
- 解析对齐 C 版：`# <iface> <duid> <iaid> <hostname> ... <addr/prefix>`，主机名取偏移后第 4 列，
  地址取最后一列并去掉 `/prefix`；注释行、字段不足、占位主机名 `*` 跳过。
- 配合 `local-domain`，`<host>.<local-domain>` 与裸主机名都能解析（对齐 C 版
  `test-local-domain.cc` 的 odhcpd 用例）。
测试：`test_parse_odhcpd_line`、`test_odhcpd_store_lookup`、`test_odhcpd_lease_with_local_domain`。

## 12. 第八批实现：cache-mem-size

- `DnsCache` 维护估算占用（结构体 + 名称长度 + 每条记录 128B 常量）；
  `cache-mem-size` 超限后按分片轮转淘汰 LRU，直到回到预算内；0 表示不限制（C 版默认）。
- 覆盖三个插入路径（正向、否定、持久化加载）与 `clear()` 的重置。
- 对外新增 `DnsCache::used_bytes()` 便于观测。
测试：`dns_mw_cache::tests::test_cache_mem_size_evicts_entries`。
