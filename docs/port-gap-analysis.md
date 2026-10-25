# Rust 移植版 vs C 原版：功能缺口评审

评审对象：`pymumu/smartdns`（C，`dfc9ef4`）vs 本仓库 `smartdns-rs`（基线 v0.13.5）。
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
（`src/config/parser/`）支持 127 个键。逐项比对后，**C 有而 Rust 无的共 37 个（已补齐 31 个；group-end 为提取误报，实际已支持；其余 5 项为不移植）**：

| 分类 | 指令 | 影响 | 状态 |
| :--- | :--- | :--- | :--- |
| 静态记录 | `txt-record` | 自定义 TXT 记录 | **本次已实现** |
| 静态记录 | `srv-record`（增强） | C 版支持多组、Rust 单值 | **本次已实现（多值累积）** |
| 缓存 | `cache-mem-size` | 缓存内存上限 | **本次已实现（估算占用 + 超限 LRU 淘汰）** |
| 缓存 | `serve-expired-prefetch-time` | 过期前预取窗口，`src/dns_server/cache.c:87` | **本次已实现** |
| 并发/网络 | `max-query-limit` | 并发查询上限，超出拒绝，`src/dns_server/dns_server.c:483` | **本次已实现** |
| 并发/网络 | `socket-buff-size` | socket 收发缓冲区 | **本次已实现（bind 前设置 SO_RCVBUF/SO_SNDBUF）** |
| 行为 | `force-no-CNAME` | 应答中去掉 CNAME 只留地址，`src/dns_server/address.c:195`、`answer.c:64` | **本次已实现** |
| 行为 | `local-domain` | 本地域后缀走 mDNS 组 + hosts 条目后缀展开 | **本次已实现** |
| PTR | `local-ptr-enable` | 用 hosts 回答本机地址的 PTR（Rust 已有 `LocalPtrZoneProvider`，缺开关） | **本次已实现** |
| 防火墙 | `ipset` / `ipset-timeout` / `ipset-no-speed` | ipset 下发与超时 | **本次已实现（Linux 调用 ipset CLI）** |
| 防火墙 | `nftset-timeout` / `nftset-debug` / `nftset-no-speed` | nftset 超时/调试/不可达下发 | **本次已实现** |
| 日志 | `log-syslog` / `audit-syslog` | syslog 后端（unix） | **本次已实现** |
| 日志 | `audit-SOA` / `audit-console` | SOA 应答也审计 / 审计同时打印到控制台 | **本次已实现** |
| 证书 | `bind-cert-generate` / `-root-key-file` / `-san` / `-validity-days` | 自签证书自动生成 | **本次已实现** |
| 运维 | `no-daemon` / `no-pidfile` / `restart-on-crash` | 守护进程行为 | 不移植（交给 systemd/Windows 服务） |
| 运维 | `data-dir` | 数据目录（相对 cache-file 按此解析） | **本次已实现** |
| 运维 | `debug-save-fail-packet` / `-dir` | 失败请求报文落盘 | **本次已实现** |
| ACL | `ip-rules` | 按客户端 CIDR 的上游过滤规则 | **本次已实现** |
| ACL | `acl-enable` | 以 client-rules 为白名单，未匹配客户端 REFUSED | **本次已实现** |
| 其它 | `odhcpd-lease-file` | OpenWrt odhcpd 租约解析 | **本次已实现** |
| 其它 | `group-match` | 组匹配语法糖（域名 / 客户端 → 组） | **本次已实现** |
| 其它 | `ddns-domain` / `plugin` | DDNS、插件框架 | 不移植 |

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

## 4. 收尾状态

§1 表格中所有可移植项均已实现并带测试（见 §5 起的分批记录）；剩余只有 §15 列出的
`not-applicable` 项。逐项的启用写法与取值建议见 §22。

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

## 6. 剩余项（仅剩不移植）

`acl-enable` / `ip-rules`（§21、§19）与 `log-syslog` / `audit-syslog`（§18）后续均已实现；
不移植项见 §15。

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

## 13. 第九批实现：debug-save-fail-packet / -dir

- 新增最外层 `DnsFailPacketMiddleware`：链路返回 Err 且开关打开时，把请求报文写成
  `<dir>/client-<qname>-<unix>.pkt`；默认目录 `/tmp/smartdns`（对齐 C 版）。
- 写盘走 `spawn_blocking`，不拖慢失败响应；序列化或写盘失败只记 warn。
测试：`dns_mw_debug::tests::test_write_fail_packet`、`test_parity_directives`。

## 14. 第十批实现：group-match

C 版中 `group-match` 只是语法糖：`-d` 等价 `nameserver /domain/group`，`-c` 等价
`client-rules <cidr> -group <group>`。Rust 侧解析后直接翻译为 `ForwardRule` 与 `ClientRule`，
不引入新的运行时逻辑。支持 `-g/--group`、`-d/--domain`、`-c/--client-ip` 短长两种写法。
测试：`config::parser::group_match::tests::test_parse`、`dns_conf::tests::test_group_match_sugar`。

## 15. 不移植项（`not-applicable`）

原计划中唯一带阻塞点的 `acl-enable` 已在 §21 实现（语义为白名单 + 未匹配 REFUSED）。

不移植（`not-applicable`）：

- `no-daemon` / `no-pidfile` / `restart-on-crash`：本实现从不 daemonize，进程管理与重启交给 systemd / Windows 服务。
- `plugin`：C 版插件 ABI（`.so` 回调）不可移植到 Rust。
- `ddns-domain`：独立 DDNS 功能，超出 DNS 解析器移植范围。

## 16. 第十一批实现：nftset-no-speed

- `nftset-no-speed <set 规格>`：在 nftset 中间件完成正常下发后，对本次应答的每个 IP 做一次
  可达性探测（ICMP，2s 超时），不可达者写入配置的 set。仅在配置该指令时执行探测。
- 与 C 版的差异：C 复用测速阶段的逐 IP 结果；Rust 现有 `ping_fastest` 只返回最快 IP，
  不保留失败列表，因此这里单独探测（只在启用该指令时有开销），行为一致。
测试：`test_parse_nftset_no_speed_list`、`test_parity_directives`；Linux 门控代码已做去门控类型检查。

## 17. 第十二批实现：bind-cert-generate 家族

- `bind-cert-generate yes`：未配置证书的 TLS/DoH/DoQ/DoH3 监听在启动时生成（或复用）
  自签证书，落在 `data-dir`（未配置时临时目录），文件名固定。
- `bind-cert-san`：逗号分隔的额外 SAN，自动附带 `localhost` / `127.0.0.1` / `::1`。
- `bind-cert-validity-days`：生成证书的有效期（`not_before=now`）。
- `bind-cert-root-key-file`：用该 PEM 密钥作为 CA 签发；未配置则自签。
实现：新增 `src/rustls_self_signed.rs`（rcgen 0.14），接入 `server::create_cert_resolver`。
测试：`rustls_self_signed::tests::test_generate_self_signed`、`test_parity_directives`。

## 18. 第十三批实现：log-syslog / audit-syslog

- `log-syslog yes`：在 unix 上把 tracing 输出额外转发到 syslog（LOG_DAEMON）；
  非 unix 平台启动时会打印一条 warn 说明不支持。
- `audit-syslog yes`：审计记录改发 syslog（需同时开启 `audit-enable`）。
- 依赖 `syslog = "7"` 仅在 `cfg(unix)` 下拉取；API 已对照源码核对。
测试：`test_parity_directives`；cargo test 338+10+7 全绿，clippy 零告警。

## 19. 第十四批实现：ip-rules

- 语法：`ip-rules <cidr> [-blacklist-ip|-whitelist-ip|-bogus-nxdomain|-ignore-ip]`，可组合多个 flag。
- 运行时：`dns_mw_ns` 按 `req.src().ip()` 用 `RuntimeConfig::ip_rule_sets_for` 把命中规则的网段
  合并进本次请求的 ignore/blacklist/whitelist 集合（无匹配时零拷贝复用全局 `Arc`）；
  `dns_mw_bogus` 用 `ip_rule_bogus_set_for` 处理客户端级 `-bogus-nxdomain`。
测试：`config::parser::ip_rule::tests::test_parse`、`dns_conf::tests::test_ip_rules_per_client`。

## 20. 第十五批实现：ipset / ipset-timeout / ipset-no-speed

- `ipset /domain/<set>`：域级规则，可多条累积；应答中的 A/AAAA 通过 `ipset add <set> <ip> -exist`
  下发（Linux；非 Linux 为空操作）。
- `ipset-timeout yes`：条目超时取答案 TTL * 3。
- `ipset-no-speed <set>`：应答 IP 做可达性探测，不可达者写入该 set。
实现：新增 `src/dns_mw_ipset.rs`（与 nftset 中间件对称，Linux 注册），`DomainRule.ipsets` 走
RuleGroup → DomainRuleMap 链路。
测试：`dns_mw_ipset::tests::test_build_add_args`、`config::parser::ipset_name::tests::test_parse`、
`test_parity_directives`。

## 21. 第十六批实现：acl-enable

C 版语义：ACL 是白名单——开启后未匹配任何 `client-rules` 的客户端返回 REFUSED。
Rust 侧在 `DnsMiddlewareHandler::search` 中判定（后台预取豁免），错误以 `ResponseCode::Refused`
透传，`app.rs` 的错误分支现在会保留携带响应码的错误而不再统一覆盖为 ServFail。
测试：`dns_mw::tests::test_acl_refuses_unmatched_client`、`test_parity_directives`。

## 22. 补齐项的启用写法与取值建议

语法均取自 `src/dns_conf::tests::test_parity_directives` 与各 parser 用例，可直接粘贴。
「进默认配置」= 是否建议写进 `etc/smartdns/smartdns.conf`（发行包与 `service install` 用的那份）。

### 22.1 建议进默认配置（低风险、收益直接）

```conf
# 并发上限：溢出直接 REFUSED，避免上游被打爆时本机堆积。默认已是 65535，显式写出便于容量规划
max-query-limit 4096

# 缓存内存上限（字节，无单位后缀）：0 为不限。8 万条量级约 32 MiB，路由器等小内存设备务必设置
cache-mem-size 33554432

# 过期预取窗口（秒）：剩余 TTL 低于该值即后台刷新。默认 0 时沿用 600s
serve-expired-prefetch-time 300

# 相对 cache-file 等相对路径的基准目录；不写则用进程工作目录
data-dir /var/lib/smartdns
```

- 高吞吐/易受攻击场景才需要下调 `max-query-limit`；一般家用 4096 足够，写太小会误伤并发下载资源页的场景。
- `cache-mem-size` 按估算占用统计（结构体 + 名称长度 + 每条记录 128 B），超限按分片轮转淘汰 LRU，
  与 `cache-size`（条数上限）叠加生效，谁先到谁生效。
- `data-dir` 只影响**相对**路径解析；已经是绝对路径的 `cache-file` 行为不变。

### 22.2 日志与审计（排障用，默认保持关闭）

```conf
# 审计默认关闭；开启后记录每条查询
audit-enable yes
audit-file /var/log/smartdns/audit.log
audit-size 131072          # 单文件字节上限
audit-num 5                # 轮转保留份数
audit-SOA no               # 只有 SOA、无地址记录的应答不写审计（对齐 C 默认，减少噪声）
# audit-console yes        # 同时打到控制台，只在临时抓现场时开
# audit-syslog yes         # 审计改发 syslog（仅 unix，且需 audit-enable）
# log-syslog yes           # tracing 日志额外转发 syslog（仅 unix；Windows 启动时打印一条不支持告警）
# log-color no             # 默认开启着色；重定向到文件或 journald 时可关
```

- Windows / macOS 上 `log-syslog`、`audit-syslog` 无 syslog 后端，别写进跨平台默认配置。
- `audit-console yes` 会把查询域名写进服务标准输出，systemd/容器场景等于把隐私写进日志采集面。

### 22.3 本地域名与租约（OpenWrt / 局域网）

```conf
local-domain lan
dnsmasq-lease-file /var/log/dnsmasq.leases
odhcpd-lease-file /tmp/odhcp6c-leases    # 可与 dnsmasq 租约并存，两者合并到同一份映射
local-ptr-enable yes                     # 默认开启；关闭后本机地址 PTR 回落上游
```

- `local-domain` 有两条独立效果：① hosts/租约里 `<host>.lan` 与裸主机名互相命中；
  ② `.lan` 后缀查询路由到 `mdns` 上游组——**②需要额外 `mdns-lookup yes`**（默认关闭），
  否则 `mdns` 组为空，这类查询直接走普通上游。只想用①时不要顺手打开 `mdns-lookup`。
- odhcpd 行格式按 C 版夹具对齐：`# <iface> <duid> <iaid> <hostname> ... <addr/prefix>`，
  主机名为第 4 列、地址取最后一列并剥掉 `/prefix`；`*` 占位主机名与注释行跳过。
- 任一租约文件 mtime 变化即重载，不需要重启服务。

### 22.4 防火墙下发（Linux 专属）

```conf
# nftset：set 规格为 <family>#<table>#<set>，family 取 ip / ip6 / inet
nftset /example.com/#4:ip#filter#dns_v4,#6:ip6#filter#dns_v6
nftset-timeout yes            # 元素超时 = 应答 TTL * 3（原实现恒为 0，即永不过期）
nftset-debug yes              # 逐条打印下发的 IP、set 名与超时，只在排查下发失败时开
nftset-no-speed #4:ip#filter#nospeed

# ipset：调用 ipset CLI，set 必须预先存在且地址族匹配（v4 集与 v6 集要分开写规则）
ipset /example.com/dns_v4
ipset /example.com/dns_v6
ipset-timeout yes             # 条目超时 = 应答 TTL * 3
ipset-no-speed dns_nospeed
```

- 两者都只在 Linux 生效：`nftset` 还需 feature `nft`（默认 `common` 已含），`ipset` 中间件仅在
  配置了 `ipset` 规则或 `ipset-no-speed` 时才注册。
- `*-no-speed` 会在后台任务里对本次应答的每个 IP **逐个**做 ICMP 可达性探测（每个 2 s 超时，
  最坏 N × 2 s，但串行在 `tokio::spawn` 内，不拖慢给客户端的应答），不可达者写入配置的 set。
  **只在配置该指令时**才有这份开销；ICMP 无权限的环境（容器、非 root 服务）或 feature
  `disable_icmp_ping` 下不会生效。
- 元素超时建议保持开启：关闭时条目永久驻留，域名换 IP 后 set 里仍是旧地址。

### 22.5 客户端维度的分组与过滤

```conf
# 白名单：未匹配任何 client-rules 的客户端返回 REFUSED（后台预取豁免）
acl-enable yes
client-rules 192.168.1.0/24
client-rules 192.168.100.0/24 -group office

# 按客户端网段附加 IP 过滤，可组合多个 flag
ip-rules 192.168.1.0/24 -blacklist-ip
ip-rules 10.0.0.0/8 -bogus-nxdomain -ignore-ip

# 组匹配语法糖：等价于 nameserver /domain/group + client-rules <cidr> -group <group>
group-match -g office -d example.com -c 192.168.1.0/24
```

- `acl-enable yes` 前务必先把所有合法客户端段写全，否则开启瞬间会拒掉整个网段——包括本机回环
  之外的设备。建议先在日志里确认 `client-rules` 命中情况再切。
- `ip-rules` 只在请求源 IP 命中 CIDR 时把对应集合并入本次查询；无匹配时复用全局集合，零额外拷贝。
- `group-match` 不引入新运行时路径，纯展开，和手写两条指令等价；同一份配置里两种写法混用没问题。

### 22.6 加密监听与证书

```conf
bind-tls [::]:853
bind-https [::]:8443
bind-quic [::]:784
bind-h3 [::]:443

# 未显式配置证书时自动生成/复用自签证书（落在 data-dir，未配置则临时目录）
bind-cert-generate yes
bind-cert-san dns.example.com,192.0.2.10
bind-cert-validity-days 30
# bind-cert-root-key-file /etc/smartdns/ca-key.pem    # 用该 PEM 私钥作为 CA 签发，否则自签
```

- 自签只适合内网/测试：客户端仍需信任或跳过校验。生产仍应显式指定证书
  （`bind-tls 853 -server-name dns.example.com -ssl-certificate /path/dns.example.com.crt -ssl-certificate-key /path/dns.example.com.key`），
  `bind-cert-generate` 作为「证书还没申请下来也能先跑起来」的兜底。
- `data-dir` 未配置时自签证书落在系统临时目录，重启容器/清理 tmp 会重新生成，导致客户端指纹变化。

### 22.7 上游协议与静态记录

```conf
server-http3 dns.example.com                      # 与 server-h3 等价，需 feature dns-over-h3（默认已含）
socket-buff-size 262144                           # 监听 socket SO_RCVBUF/SO_SNDBUF，UDP 高 QPS 时防丢包

txt-record /example.com/v=spf1 -all               # 同域名多行累积为同一个 TXT RRset
txt-record /example.com/notice="maintained by ops"
srv-record /_xmpp-client._tcp/xmpp.example.com,5222,0,5
srv-record /_xmpp-client._tcp/xmpp2.example.com,5223,10,1   # 多值累积，不再互相覆盖
force-no-CNAME no                                 # 开启后应答裁掉 CNAME 只留地址（CNAME 类型查询本身不裁）
```

- ⚠️ `server-http3` / `server-h3` 只在**没写 scheme** 时补默认 `h3://`。写成
  `server-http3 https://doh.pub/dns-query` 会保留 `https`，静默变成一条 DoH 上游——
  要么写裸主机名，要么显式 `h3://`。
- ⚠️ `srv-record` 的值顺序是 `<target>,<port>,<priority>,<weight>`，**不是** C 版的
  `priority weight port target`；写错顺序会得到可用的记录但优先级/端口全反。
- `audit-size` / `log-size` 走 `Byte` 解析，支持 `128k`、`16m` 这类后缀；
  `cache-mem-size` 是纯整数（字节），带后缀会解析失败。

- `socket-buff-size` 在 bind 前设置，对 UDP/TCP/DoT/DoH/DoH3/DoQ 全部监听器生效；取值受内核
  `net.core.rmem_max`/`wmem_max` 上限约束，超过会被静默截断到上限。
- `force-no-CNAME` 会破坏依赖 CNAME 链的客户端（部分 CDN 健康检查、`dnsmasq` 转发校验），
  默认关闭；只有确认下游只看地址时才开。

### 22.8 排障专用（不要长期开启）

```conf
debug-save-fail-packet yes
debug-save-fail-packet-dir /var/tmp/smartdns-fail   # 默认 /tmp/smartdns
```

- 失败请求报文写入 `<dir>/client-<qname>-<unix>.pkt`，目录需存在且服务账号可写。
- 默认 `/tmp/smartdns` 在 Windows 上会落到当前盘符根目录，跨平台部署请显式指定目录。
- 长期开启会持续产生小文件、无自动清理，且文件名含查询域名（等同把查询记录落盘）。确认问题后关掉。

### 22.9 已知未生效项

- `bind-cert-key-pass`：能被解析并存进配置（`RuntimeConfig::bind_cert_key_pass`），但加载私钥时
  不会用它解密，属于**写了也没用**的指令。加密私钥请先自行解密成明文 PEM，再配 `bind-cert-key`。
  其余 C 版指令均已按语义接线；本项也是 `TODO.md` 中唯一未勾选项。
