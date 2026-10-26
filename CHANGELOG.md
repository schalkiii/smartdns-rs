# Changelog

本文件记录 smartdns-rs 的变更历史。格式遵循 [Conventional Commits](https://www.conventionalcommits.org/)。

## [Unreleased]

## [v0.13.6] - 2026-10-26

### test: 解锁并补上 cname 改写 → address 规则链路用例

- 定位到此前失败的两个根因，均非产品缺陷：夹具用相对名建键（规则改写后为 FQDN，hickory 区分二者）、
  测试链序与生产相反（生产是 cname 在外、addr 在内）。
- 用例按生产顺序组装，断言答案含 CNAME 记录且 address 规则覆盖上游地址；mock 记录改用全限定名。
- 测试总量 355 + 10 + 7。

### test: 双栈切换判定补齐「无结果」分支

- 从 `which_faster` 抽出纯函数 `decide_that_faster(this, that, threshold)`：对方无结果不切换、
  本族无结果且对方有则切换、双方都有结果时才比较阈值；补 4 条边界用例。
- 判定层至此全覆盖（`is_that_faster` 4 例 + `decide_that_faster` 4 例）；仅剩 ICMP 采样与超时循环
  依赖真实网络，已在 `docs/TEST-MATRIX.md` G4 记录。
- 测试总量 354 + 10 + 7。

### test: 补 server socket 绑定用例

- `server::net` 新增 2 例：端口 0 时 UDP/TCP 均绑定成功（含 `socket-buff-size` 路径）、
  端口被占用时返回 Err 而不是 panic（回归 R2 的 `bind_to` 行为）。
- 尝试补 zone 身份查询用例未通过（`server-name` TXT 返回 NoRecordsFound），已回退，
  原因与复现条件记入 `docs/TEST-MATRIX.md` 的阻塞章节。
- 测试总量 353 + 10 + 7。

### test: 补 bogus-nxdomain 用例并产出覆盖率矩阵

- `test_bogus_nxdomain_filters_mocked_ip`：命中 `bogus-nxdomain` 的 A 记录整条应答转 NXDOMAIN，
  未命中的照常返回（顺带覆盖 `LookupError::response_code()`）。
- 新增 `docs/TEST-MATRIX.md`：按目录给出文件/行数/用例/零测试文件数，列出 7 类关键缺口、
  本轮新增用例清单、已知阻塞（含证据）与复现命令。
- 测试总量 351 + 10 + 7；零测试源文件 79 → 77。

### test: 覆盖并发上限判定与双栈切换判据

- `src/app.rs`：把 `max-query-limit` 的判定抽成 `should_refuse_query`，补 4 条用例
  （0=不限、等于上限放行、严格超过才拒绝、后台预取豁免）。
- `src/dns_mw_dualstack.rs`：把「另一地址族明显更快」抽成 `is_that_faster`，补 4 条边界用例
  （差值必须严格大于 `dualstack-ip-selection-threshold`）。
- 测试总量 350 + 10 + 7；仍未覆盖的部分：并发门的真实链路（需完整 App 装配）、
  dualstack 的 ping 注入（现依赖真实 ICMP，CI 下被禁用）、跨中间件端到端用例
  （`DnsMockMiddleware` 以 `query.original()` 取键，cname 改写后的查询无法命中）。

### fix(cache): rr-ttl / rr-ttl-min / rr-ttl-max 同时作用于缓存 TTL

C 版在**入库时**就用 `_dns_server_get_conf_ttl` 计算缓存 TTL（`cache.c:140`），本移植版此前只在回复时
夹取，缓存存的是上游原始 TTL：开 `rr-ttl-min` 时条目比配置更早过期（命中率低、上游查询多），
开 `rr-ttl-max` 时长 TTL 记录会突破上限继续驻留。

现在抽出 `rr_ttl_bounds` / `clamp_ttl_bounds`（`src/dns_mw_addr.rs`），addr（回复）与 cache（入库）共用；
`rr-ttl-reply-max` 维持只压回复、不参与缓存寿命（与 C 一致）。新增 2 个单测覆盖边界折叠与夹取顺序。
测试：348 + 10 + 7 全绿，clippy 0，fmt 0。

### fix(dns): 校验 ECS 前缀，伪造前缀不再触发请求任务 panic

hickory 的 `EcsOption` 解析把 `source_prefix` 当裸 `u8` 读入、不做范围校验，客户端可以给出
> 32（v4）/ > 128（v6）的值；原实现直接 `Ipv4Net::new(..).unwrap()`，会 panic 掉该请求任务。
现改为经 `ipnet_from_subnet` 返回 `Option`，非法前缀按「未携带 ECS」处理、回落客户端源 IP。
新增 3 个单测：合法 v4/v6、非法 33/129、IPv4-mapped IPv6。

### fix(infra): 缓存持久化写锁中毒不再连锁 panic

`src/infra/mapped_file.rs` 的 `Mutex::lock().unwrap()` 改为 `unwrap_or_else(|e| e.into_inner())`，
写线程 panic 后后续 checkpoint 仍能继续（与 `dns_client.rs` 既有的毒性恢复写法一致）。

### docs: 新增 v0.13.5 代码审计报告，并补齐接口安全说明

- `docs/AUDIT-REPORT-2026-10-25.md`：七维排查（文档一致性/缺漏、架构、代码写法、测试覆盖与严谨性、工程规范）
  + 问题清单 + 逐条建议 + 优先级与并行性，末尾附处置状态表；含「未做穷尽逐函数比对」的覆盖声明。
- 两份 README：Web UI 段落补出可写接口清单（缓存清空、配置重载、address 增删改）与「置于反代+认证之后」的
  建议；预取批量/间隔的表述校正为环境变量 `PREFETCH_MAX_BATCH` / `PREFETCH_MIN_INTERVAL`（不是 conf 选项）。

### docs: 记录配置静默忽略行为与瞬时 SERVFAIL 症状

- `AGENTS.md` 新增「不认识的指令会被静默吞掉」：`RuntimeConfig::config()` 对未知键和非法取值不报错
  （`dns_conf.rs` 的 `Ok((_, None)) => ()`），并给出 `usize` 类与 `Byte` 类数值键的后缀差异、
  哪些写法属于空写，以及为什么不能用 `/api/config` 或「启动没报错」判断生效。
- `AGENTS.md` 的 Linux 门控段落补上 `dns_mw_ipset`：模块声明无条件编译，但 ipset CLI 下发在
  `#[cfg(target_os = "linux")]` 分支里，Windows 同样检不到。
- `README.md` / `README_zh-CN.md` 故障排查新增 `no connections available` 小节：亚毫秒级失败的观测特征、
  可排除的原因（不是上游组为空，也不是版本回归，v0.13.2 同配置同样复现），根因标注为未定位。
- `README_zh-CN.md` 补齐英文版已有的「与 C 版配置对齐」特性条目，恢复两份 README 的内容一致。
- `docs/port-gap-analysis.md` §22.7：`socket-buff-size` 补上「只接受纯整数，写 `512K` 被静默忽略」，
  并去掉与 `cache-mem-size` 重复的 `Byte` 后缀说明。
- `docs/port-gap-analysis.md` §22.1：说清 `cache-mem-size` 与 `cache-size` 是两条独立上限、谁先到谁生效，
  内存充裕建议直接写 0；本机 2026-10-25 实测常驻 222 条时 32 MiB 上限未触顶，命中率下降与该上限无关。

### docs: 同步 C 版指令对齐结论与逐项启用建议

- `docs/port-gap-analysis.md` 新增 §22：按主题给出全部补齐项的可粘贴写法、取值建议、平台/feature
  限制与「是否建议进默认配置」；§1 表格里 `socket-buff-size`、`nftset-no-speed` 两行状态已过期，改为已实现。
- 记录两处易踩的坑：`srv-record` 的值顺序是 `<target>,<port>,<priority>,<weight>`（不是 C 版顺序）；
  `server-http3`/`server-h3` 只在未写 scheme 时补默认值，写 `https://...` 会静默退化成 DoH。
- `TODO.md`：勾掉 12 项实际已实现的指令（`ipset*` / `nftset*` / `dns64` / `server-quic` /
  `serve-expired-prefetch-time` / `dualstack-ip-selection*`），并补一节 v0.13.5 补齐清单。
- `README.md`：新增「Configuration parity with the C version」特性条目并指向缺口评审文档。

### ci: 暴露 crates.io 发布失败，不再静默

- `version.yml` 的 `Publish to crates.io` 带 `continue-on-error: true`，失败时 job 仍是绿的。
  补一个 `steps.publish.outcome == 'failure'` 的 warning 注解步骤，避免「以为发到 crates.io 了」。
- 实测 v0.13.5 该步失败原因：`hickory-proto` / `hickory-resolver` / `rustls-pki-types` 只有 git 依赖
  （fork 补丁，crates.io 无对应版本），`cargo publish` 在 manifest 校验阶段即报错；本 fork 也未配置
  `CARGO_REGISTRY_TOKEN`。`AGENTS.md` 的 Release 段落据此改写，不再声称发布到 crates.io。

## [v0.13.5] - 2026-10-25

### feat(conf): 支持 acl-enable 客户端白名单

- 开启后未匹配任何 `client-rules` 的客户端返回 REFUSED（后台预取豁免）。
- 错误处理保留携带的 DNS 响应码，不再统一覆盖为 ServFail。
测试：`test_acl_refuses_unmatched_client`、`test_parity_directives`；cargo test 343+10+7 全绿。

### feat(firewall): 支持 ipset / ipset-timeout / ipset-no-speed

- `ipset /domain/<set>`：应答 IP 通过 `ipset add ... -exist` 下发（Linux，调用 ipset CLI）。
- `ipset-timeout`：条目超时取 TTL*3；`ipset-no-speed`：探测不可达的 IP 写入指定 set。
实现见 `src/dns_mw_ipset.rs`，与 nftset 中间件对称。
测试：`test_build_add_args`、`test_parse`、`test_parity_directives`；cargo test 342+10+7 全绿。

### feat(conf): 支持 ip-rules 按客户端 CIDR 的过滤规则

`ip-rules <cidr> [-blacklist-ip|-whitelist-ip|-bogus-nxdomain|-ignore-ip]`，可组合多个 flag。
按请求源 IP 合并进 ignore/blacklist/whitelist 集合（无匹配时零拷贝复用全局集合）；
bogus-nxdomain 在响应阶段按客户端规则过滤。
测试：`test_parse`、`test_ip_rules_per_client`；cargo test 340+10+7 全绿。

### feat(log): 支持 log-syslog 与 audit-syslog

- `log-syslog`：unix 上把日志额外转发到 syslog；非 unix 打印一条不支持告警。
- `audit-syslog`：审计记录改发 syslog（需开启 audit-enable）。
- `DnsAuditMiddleware::new` 参数收敛为 `AuditOptions`，消除 too_many_arguments 告警。
测试：`test_parity_directives`；cargo test 338+10+7 全绿，clippy 0。

### feat(tls): 支持 bind-cert-generate 自签证书家族

- `bind-cert-generate`：未配置证书时自动生成/复用自签证书（落在 data-dir）。
- `bind-cert-san`：额外 SAN（自动带 localhost/127.0.0.1/::1）。
- `bind-cert-validity-days`：证书有效期。
- `bind-cert-root-key-file`：用 PEM 根密钥作为 CA 签发，否则自签。
实现见 `src/rustls_self_signed.rs`（rcgen 0.14），接入 `create_cert_resolver`。
测试：`test_generate_self_signed`、`test_parity_directives`；cargo test 338+10+7 全绿。

### feat(nft): 支持 nftset-no-speed

- 配置 `nftset-no-speed <set>` 后，应答中的 IP 会做一次可达性探测，不可达者写入该 set。
- 只在启用该指令时探测；Linux + feature nft 下生效。
测试：`test_parse_nftset_no_speed_list`、`test_parity_directives`；cargo test 336+10+7 全绿。

### feat(conf): 支持 group-match 组匹配语法糖

`group-match -g <group> -d <domain> -c <cidr>` 翻译为既有的 `nameserver /domain/group`
与 `client-rules <cidr> -group <group>`，不新增运行时路径。
测试：`test_parse`、`test_group_match_sugar`；cargo test 335+10+7 全绿。

### feat(debug): 支持 debug-save-fail-packet / -dir

- 查询失败时把请求报文写入 `<dir>/client-<qname>-<unix>.pkt`（默认目录 `/tmp/smartdns`）。
- 最外层中间件捕获全链路失败；写盘走阻塞线程，序列化/写盘失败只记 warn。
测试：`test_write_fail_packet`、`test_parity_directives`；cargo test 333+10+7 全绿。

### feat(cache): 支持 cache-mem-size 缓存内存上限

- `cache-mem-size [bytes]`：按估算占用（结构体 + 名称 + 每条记录保守常量）统计，
  超过上限时按分片轮转淘汰 LRU 条目；0 表示不限制（对齐 C 版默认）。
- 覆盖正向/否定/持久化加载三条插入路径，`clear()` 同步重置计数。
- 新增 `DnsCache::used_bytes()` 便于观测。
测试：`test_cache_mem_size_evicts_entries`；cargo test 332+10+7 全绿。

### feat(conf): 支持 odhcpd-lease-file，与 dnsmasq 租约合并

- `odhcpd-lease-file`：读取 OpenWrt odhcpd DHCPv6 租约，与 dnsmasq 租约合并到同一份映射；
  任一文件 mtime 变化即重载。解析规则对齐 C 版（`# <iface> <duid> <iaid> <host> ... <addr/prefix>`）。
- 可由 odhcpd 单独提供租约（未配置 dnsmasq 租约时也可用）。
- 与 `local-domain` 联动：`<host>.<local-domain>` 与裸主机名均可解析。
测试：`test_parse_odhcpd_line`、`test_odhcpd_store_lookup`、`test_odhcpd_lease_with_local_domain`。

### feat(conf): 支持 local-domain，并稳定网络型测试

- `local-domain <domain>`：该后缀下的查询走 mDNS 服务组（与 C 版注册 local/mdns 组一致）；
  hosts 查询自动剥离后缀命中基数名并改写记录名，等价于 C 版给 hosts 条目展开后缀；
  `local-domain -` 清除设置。
- `dns_client::tests::test_with_default`、`dns_mw_ns::tests::test_edns_client_subnet` 依赖真实外网，
  在并行负载下偶发超时（CI 偶发红的原因之一），标记 `#[ignore]`，需要时 `cargo test -- --ignored`。
测试：`config::parser::local_domain::tests::test_parse`、`dns_mw_hosts::tests::test_local_domain_base`、
`test_parity_directives`；cargo test 328+10+7 全绿（11 ignored），clippy 零告警，fmt 干净。

### feat(conf): 补齐 server-http3、data-dir、log-color 与审计开关

- `server-http3`：`server-h3` 的等价别名（h3://）。
- `data-dir`：相对 `cache-file` 以此目录为基准；绝对路径不变。
- `log-color`：控制台日志 ANSI 着色，默认开启（对齐 C 版默认）。
- `audit-SOA`：默认关闭；只有 SOA、没有地址记录的应答不写审计（C `audit.c:127`）。
- `audit-console`：审计记录同时打印到控制台（C `audit.c:200`）。
测试：`test_parity_directives`、`test_data_dir_resolves_relative_cache_file`、
`test_config_nameserver`；cargo test 328+10+7 全绿，clippy 零告警，fmt 干净。

### feat(conf): 补齐 nftset 超时/调试与监听 socket 缓冲配置

- `nftset-timeout`：开启后 nftset 元素带 `TTL * 3` 的超时（原来恒为 0，即永不过期），
  对应 C 版 `context.c:672`。
- `nftset-debug`：打开后逐条记录下发的 IP、set 名与超时。
- `socket-buff-size`：监听 socket 在 bind 前设置收发缓冲区，UDP/TCP/DoT/DoH/DoH3/DoQ
  全部监听器生效。
- 顺带把 nftset 中间件里 v4/v6 重复的下发循环收敛成 `add_addrs`。
测试：`test_parity_directives` 覆盖两个新指令；gating 用 `feature = "nft"`。

### feat(conf): srv-record 支持同域名多值累积

`srv-record` 同一域名写多行时，原实现后者覆盖前者，只剩最后一条 SRV；C 版是把它们
追加到同一规则、作为同一个 SRV RRset 返回。现改为 `DomainRule.srv: Option<Vec<SRV>>`
并累积，`ZoneProvider` 一次返回全部记录，查询类名与 TTL 保持与单条时一致。

测试：`dns_mw_zone::tests::test_srv_record_multiple`（断言两条记录的目标名都在）。
`cargo test --bin smartdns srv` 通过。

### feat(conf): 补齐并发限制、CNAME 裁剪、本地 PTR 开关与过期预取窗口

继续对照 C 原版补齐配置面，四项均已按原版语义接线并测试：

- `max-query-limit`（默认 65535，0 为不限）：并发请求数超过上限时直接回 `REFUSED`，
  不进入处理链，告警按 120s 节流——对应 C 版 `dns_server.c:483`。落点 `app.rs` 调度循环
  与 `server::refused_response`（复用原有的 REFUSED 应答构造）。
- `force-no-CNAME`：新增最外层 `DnsForceNoCnameMiddleware`，应答中裁掉 CNAME 记录只保留
  解析结果；`CNAME` 类型查询本身的答案不裁。
- `local-ptr-enable`（默认开启）：关闭后不再注册 `LocalPtrZoneProvider`，本机地址的 PTR
  查询回落到上游，对应 C 版 `ptr.c:269`。
- `serve-expired-prefetch-time`：命中即预取的剩余 TTL 阈值改为读配置，未配置时沿用原有
  600s 默认，避免改变默认行为。

同时把 C 版配置指令普查的剩余项重排了优先级，详见 `docs/port-gap-analysis.md`
（37 项已补 5 项，剩余 32 项）。
测试：`test_parity_directives`、`test_strip_cname_records`；`cargo test` 326 + 10 + 7 全绿，
clippy 零告警，fmt 干净。

### feat(conf): 支持 txt-record 静态 TXT 记录，对齐 C 原版行为

对照 `pymumu/smartdns`（C 原版）做配置指令普查后补齐的第一个缺口：C 版有 `txt-record`，
本移植版缺失。

- 语法与 C 版一致：`txt-record /example.com/text`；不写 `/domain/` 前缀时落到根域，
  整行作为文本（对应 C 的 `_get_domain`）。
- 同一域名的多条 `txt-record` 累积为同一个 TXT RRset，而不是后者覆盖前者
  （对应 C 的 `_conf_domain_rule_txt_record` 追加链表语义）。
- TXT 查询由 `ZoneProvider` 在本地直接应答，TTL 与其它静态记录一致（`MAX_TTL`）。
- 新增测试：`config::parser::txt::tests::test_parse`、
  `dns_conf::tests::test_txt_record_accumulates_per_domain`。
  `cargo test` 全绿（324 + 10 + 7），clippy 零告警，fmt 干净。
- 指令普查结论与后续缺口清单见 `docs/port-gap-analysis.md`（C 版 114 个指令中仍有 36 个未覆盖，
  已按影响与优先级排序）。

## [v0.13.4] - 2026-10-11

### feat(conf): 发行包与服务安装默认配置改为开箱即用

原来 `etc/smartdns/smartdns.conf` 是上游 C SmartDNS 那份几乎全是注释的模板，有效指令只有
`bind [::]:53`、`cache-size 32768`、`force-qtype-SOA 65`、`log-level info`——**一个上游服务器都没有**。
而该文件同时被 `src/main.rs` 以 `include_str!` 编成 `DEFAULT_CONF`，即 `smartdns service install`
写入系统的配置，也即发行包与 MSI 附带的那份配置，于是"解压即用"其实解析不了任何域名。

- `etc/smartdns/smartdns.conf`：改为参考当前部署配置整理的开箱即用版本——bootstrap 引导组、
  UDP 兜底（223.5.5.5 / 223.6.6.6 / 114.114.114.119）、加密上游（doh.pub / dns.alidns.com /
  cloudflare DoH / 8.8.8.8:853 DoT）、`response-mode fastest-response`、`cache-size 65536`、
  `prefetch-domain` + `serve-expired`、`force-qtype-soa 65`、`log-level info`。
- Web 管理面板默认只绑回环 `bind-http 127.0.0.1:8080`：面板无鉴权，默认不应暴露到局域网，
  README 补充了按需开放的说明与风险。
- `force-aaaa-soa` 以注释形式保留并标注适用条件（IPv6 链路差时再开），纯 IPv6 环境开启会失效。
- 原模板重命名为 `etc/smartdns/example.conf` 作为完整选项参考，`build.rs` 的模板下载目标同步改到
  该文件——否则会在新克隆时把整理好的默认配置覆盖回空模板。
- `justfile` 的 `package-prepare` 与 `web-ui.yml` 的打包清单同时附带 `example.conf`；MSI 经
  `wix.wxs` 的 `SourceDir/**` 通配自动带上，无需改动。
- 新增回归测试 `dns_conf::tests::test_shipped_confs_parse_and_default_conf_is_usable`：逐行校验两份
  配置的非注释指令都能被完整解析（无残留参数），并断言默认配置确实有监听、有上游、且为
  `fastest-response`。`cargo test` 全绿（322 lib + 10 + 7）。

### fix(ci): 修复并发发布丢资产，Windows x86_64 包重新进入 release

矩阵中的每个 build job 各自调用 `softprops/action-gh-release` 写同一个 tag，并发写同一
release 时会互相覆盖，导致部分平台的安装包没有出现在 release 上（v0.13.3 的
`smartdns-x86_64-pc-windows-msvc` 即因此缺失，当时该 job 构建、打包、artifact 全部正常）。

- `build.yml`：build job 只上传 artifact；新增 `release` job 在 `needs: build` 之后下载全部
  artifact，一次性发布到同一个 release。
- `release` job 汇总所有安装包生成 `SHA256SUMS-<version>.txt`，并从 `CHANGELOG.md` 抽取对应
  版本段落作为 release 正文；抽取不到时保留原有正文不覆盖。
- `build.yml`：目标矩阵扩为 15 个，新增并通过的有 `i686-unknown-linux-musl`、
  `armv7-unknown-linux-musleabihf`、`armv7-linux-androideabi`。另有一批目标以 `optional`
  （映射到 job 级 `continue-on-error`）试加后连续构建失败，已移出矩阵：
  `x86_64-pc-windows-gnu`（runner 缺 MinGW linker；且 `wix.wxs` 的 Package 与 Component GUID
  写死，GNU 安装包会与 MSVC 版共用 UpgradeCode 互相覆盖安装）、`riscv64gc-unknown-linux-musl`、
  `x86_64-unknown-freebsd`、`x86_64-linux-android`。`optional` 机制保留，供以后新目标试跑时隔离。
- `Cross.toml`：为通过构建的 `armv7-linux-androideabi` 固定 cross 镜像。
- `version.yml`：调用 `build.yml` 的 job 显式声明 `contents:write` + `packages:write`——
  caller 的权限是 reusable workflow 的天花板，缺失会触发 `startup_failure`。
- `version.yml`：补 workflow 级 `permissions: contents: write`。本仓库 Actions 的默认 token 权限是
  `read`，而 bump job 要把版本提交和 tag 推回 main，此前 "Push changes" 步骤必然 403
  （所以 v0.13.3 是手工 `chore(release)` 提交的，该工作流在此 fork 从未跑通过）。
- `README.md` / `README_zh-CN.md`：安装章节新增平台资产对照表与 `SHA256SUMS` 校验说明。
- `AGENTS.md`：新增 Release & Packaging 章节，固化"禁止在 build 矩阵 job 内直接发布"的约束。

> 本节为 CI/发布流程变更，不涉及运行时代码；回归测试由 release 资产清单验证
> （确认 `x86_64/i686/aarch64-pc-windows-msvc` 的 zip 与 msi 同时出现在 release 上）。

## [v0.13.3] - 2026-10-08

### feat(webui): 对标原版 smartdns WebUI——查询日志页面与上游运行统计

**新增能力**（对标原版 smartdns WebUI 的日志中心与上游统计）：

- **查询日志页面**（新导航项"查询日志"）：展示最近 1000 条查询记录（内存环形缓冲，非持久化，重启清空），支持域名/客户端/记录类型过滤、自动刷新开关（5s）与手动刷新。
- **上游服务器页面增强**：每台上游展示 IP、端口、协议（UDP/TCP/DoT/DoH/DoQ/DoH3）、安全性（加密/明文）、运行状态（正常/异常）、查询次数、成功次数、失败次数、平均耗时、成功率（进度条），并新增总查询/总成功/总失败与总体成功率汇总卡片。
- **总览页新增**：热门域名 Top 10、活跃客户端 Top 10 排行。

**后端实现**：

- `dns_client.rs`：`NameServer` 增加原子计数器（query/success/failure/total_time_ns），在查询终态（成功/ServFail/busy/超时/连接错误）记录；被组内竞速 cancel 的请求不计数，保证 `查询次数 = 成功 + 失败`。新增 `default_server_group()`/`named_server_groups()` 公开访问器。
- `dns_mw_audit.rs`：审计中间件重构为始终注册——内存环形缓冲（`QUERY_LOG_MAX=1000`）始终写入供 WebUI 使用，磁盘审计文件仅在 `audit-enable` 时写入。
- `app.rs`：`AppState` 持有 `DnsClient` 句柄与查询日志环形缓冲，重载配置时同步更新。
- 新增 API：`/api/query-log`（支持 limit/offset/domain/client/qtype 过滤）、`/api/stats/top-domains`、`/api/stats/clients`、`/api/stats/query-types`；`/api/nameservers` 响应扩展为含运行统计的完整模型。
- 前端（`webui/`）：`lib/api.ts` 新增 `useQueryLog`/`useTopDomains`/`useClients`，仪表盘新增 `querylog` 页签；`webui/out` 产物随仓库更新。

**注意**：`utoipa_axum::routes!` 多 handler 形式存在路径合并冲突（panic "Overlapping method route"），统计路由改为单 handler `routes!` 后 `merge` 的方式注册。

### ci(docker): 为 build-docker 添加 packages:write 权限，修复 ghcr.io 推送被拒

`build-docker` job 推送镜像到 ghcr.io 时报 `denied: installation not allowed to Write organization package`——workflow 级 `permissions` 仅声明 `contents: write`，缺少 `packages: write`。给 `build-docker` job 单独添加 `permissions: { contents: read, packages: write }`（job 级覆盖 workflow 级，遵循最小权限）。

### ci: 修复 cleanliness job 的 clippy/fmt 失败

- `proxy.rs`：`connect_udp` 的 HTTP 分支移除 `needless_return`（改为表达式），通过 `cargo clippy -- -D warnings`。
- 运行 `cargo fmt --all` 统一格式（address.rs / config-parser / dns_client / dns_mw_cache / server/tls），通过 `cargo fmt --check`。

### ci(build): build.yml 的 workflow_dispatch 支持 version/prerelease 输入

手动触发 `build.yml` 时可指定 `version`（git tag，如 `v0.13.2`）与 `prerelease`，配合 `softprops/action-gh-release` 发布 release；不传时回退使用分支名。

## [v0.13.2] - 2026-08-19

### build(webui): webui/out 静态产物入库，修复默认特性 CI 编译失败

`web-ui` 已加入默认特性，而 RustEmbed 编译期需要 `webui/out` 目录。将该目录从 `.gitignore` 移除并入库，保证 clone 与 CI（test/cleanliness/compatibility/build）在默认特性下可直接编译，无需先构建前端。

### fix(dns_client): Busy 快速失败，切断“busy→重连→更慢”恶性循环

**问题**：运行十余天后 WebUI “上游查询耗时”升至 700~800ms（日志实测 prefetch median=210ms / mean=222ms / p95=437ms，`resource too busy` 占查询约 32%）。

**根因**：hickory `DnsMultiplexer::send_message` 在 `active_requests` 超过 `CHANNEL_BUFFER_SIZE(32)` 时返回 `ProtoErrorKind::Busy`。此前 `is_retryable` 把 Busy 当可重试错误，重试时占用 per-NameServer 信号量槽位空耗；且 hickory `NameServerState::send` 对任意发送错误都 `set_status(Failed)`，下次查询强制重连（触发全局 200ms socket 速率限制 + TLS 握手），形成“busy → 重连 → 更慢 → 更多 busy”的恶性循环，直接推高延迟。

**修复**：新增 `is_busy()`（匹配 `ProtoErrorKind::Busy`）并从 `is_retryable()` 中剥离；`NameServer::lookup` 增加 busy 快速失败分支——不重试、不占槽等待，立即返回错误让 `NameServerGroup` 转向下一个上游。配合既有 `PER_NAMESERVER_CONCURRENCY` 信号量（<32）从源头抑制 busy。

### feat(build): web-ui 加入默认特性

`default = ["common", "self-update"]` → `default = ["common", "self-update", "web-ui"]`，默认构建即嵌入 Web 仪表盘，无需显式 `--features web-ui`。

### fix(config/api): address 托管配置回写避免 panic

**问题**：`ConfigItem::Display` 仅实现了 `Address`/`Server`/`ForwardRule` 等少数变体，其余约 70 个变体均为 `todo!()`。WebUI 地址管理 API（`api/address.rs` 的 `create`/`delete`）改写 `address.conf` 时会对整份 `ConfigFile` 调用 `format!("{config}")` 序列化写回；只要该文件包含任何非 `Address` 的配置行（如用户手改混入），即触发 `todo!()` **panic**，导致请求失败、配置写回中断。

**修复**：
- `config/parser/mod.rs`：将 `ConfigItem::Display` 全部 `todo!()` 改为安全占位（输出 `# unsupported config item: {:?}` 形式），任何变体序列化都不会 panic 并保留原始信息。
- `api/address.rs`：改写前先 `retain` 仅保留 `Address` 规则与注释/空行，确保托管文件只承载 address 规则，彻底避免触碰未实现变体。

### fix(dns_client): 空上游组 lookup 避免 panic

`NameServerGroup::lookup` 在 `servers` 为空时以 `mpsc::channel(0)` 创建容量为 0 的通道会直接 **panic**，且后续 `recv()` 无意义空等。改为在开头判断空组直接返回 `NoConnections` 错误。补充单测 `test_name_server_group_empty_lookup_no_panic` 与 `test_config_item_unsupported_display_no_panic`。

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
