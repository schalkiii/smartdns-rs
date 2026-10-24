# Rust 移植版 vs C 原版：功能缺口评审

评审对象：`pymumu/smartdns`（C，`dfc9ef4`）vs 本仓库 `smartdns-rs`（v0.13.4）。
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
（`src/config/parser/`）支持 127 个键。逐项比对后，**C 有而 Rust 无的共 37 个**：

| 分类 | 指令 | 影响 | 状态 |
| :--- | :--- | :--- | :--- |
| 静态记录 | `txt-record` | 自定义 TXT 记录 | **本次已实现** |
| 静态记录 | `srv-record`（增强） | C 版支持多组、Rust 单值 | 待评估 |
| 缓存 | `cache-mem-size` | 缓存内存上限 | 待实现（需逐条内存核算） |
| 缓存 | `serve-expired-prefetch-time` | 过期前预取窗口，`src/dns_server/cache.c:87` | 待实现 |
| 并发/网络 | `max-query-limit` | 并发查询上限，超出拒绝，`src/dns_server/dns_server.c:483` | 待实现 |
| 并发/网络 | `socket-buff-size` | socket 收发缓冲区 | 待实现 |
| 行为 | `force-no-CNAME` | 应答中去掉 CNAME 只留地址，`src/dns_server/address.c:195`、`answer.c:64` | 待实现 |
| 行为 | `local-domain` | 声明本地域，不向上游转发 | 待实现 |
| PTR | `local-ptr-enable` | 用 hosts 回答本机地址的 PTR（Rust 已有 `LocalPtrZoneProvider`，缺开关） | 待接线 |
| 防火墙 | `ipset` / `ipset-timeout` / `ipset-no-speed` | ipset 下发与超时 | 待实现（nft 已有部分能力） |
| 防火墙 | `nftset-timeout` / `nftset-debug` / `nftset-no-speed` | nftset 超时/调试 | 待实现 |
| 日志 | `log-syslog` / `audit-syslog` / `audit-console` / `log-color` | 日志后端与着色 | 低优先级 |
| 日志 | `audit-SOA` | SOA 应答也审计 | 低优先级 |
| 证书 | `bind-cert-generate` / `-root-key-file` / `-san` / `-validity-days` | 自签证书自动生成 | 低优先级（可用外部证书） |
| 运维 | `no-daemon` / `no-pidfile` / `restart-on-crash` | 守护进程行为 | 不移植（交给 systemd/Windows 服务） |
| 运维 | `data-dir` / `debug-save-fail-packet*` | 数据目录 / 失败报文落盘 | 低优先级 |
| ACL | `acl-enable` / `ip-rules` | 客户端/IP 访问控制 | 待评估 |
| 其它 | `ddns-domain` / `plugin` / `odhcpd-lease-file` / `group-match` | DDNS、插件框架、OpenWrt 租约 | 不移植 / 待评估 |

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
