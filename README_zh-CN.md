# SmartDNS-rs

![Test](https://github.com/schalkiii/smartdns-rs/actions/workflows/test.yml/badge.svg?branch=main)
[![GitHub release (latest by date including pre-releases)](https://img.shields.io/github/v/release/schalkiii/smartdns-rs?display_name=tag&include_prereleases)](https://github.com/schalkiii/smartdns-rs/releases)
![OS](https://img.shields.io/badge/os-Windows%20%7C%20MacOS%20%7C%20Linux-blue)

[Docs](https://pymumu.github.io/smartdns/) •

[English](https://github.com/schalkiii/smartdns-rs/blob/main/README.md) | 中文

SmartDNS-rs 🐋 一个是受 [C 语言版 SmartDNS](https://github.com/pymumu/smartdns)  启发而开发的，并与其配置兼容的运行在本地的跨平台 DNS 服务器，
它接受来自本地客户端的 DNS 查询请求，然后从多个上游 DNS 服务器获取 DNS 查询结果，并将访问速度最快的结果返回给客户端，
以此提高网络访问速度。 SmartDNS 同时支持指定特定域名 IP 地址，并高性匹配，可达到过滤广告的效果。

---

## 为什么选择 SmartDNS-rs？

本项目是 [SmartDNS](https://github.com/pymumu/smartdns) 的 **Rust 重写版本**，在架构和性能方面有多项改进：

| 功能 | SmartDNS (C语言) | SmartDNS-rs (Rust) |
|------|-----------------|-------------------|
| **平台支持** | 仅 Linux（其他平台需 Docker/WSL） | 原生支持 Windows、macOS、Linux、Android |
| **HTTP 服务** | 需要单独进程 | 内置异步 HTTP 服务器 |
| **并发模型** | 基于线程 | Tokio 异步运行时 |
| **内存安全** | 手动内存管理 | 编译时安全保证 |
| **配置方式** | 仅文件配置 | 文件 + REST API 热重载 |
| **Web UI** | 不包含 | 内置仪表盘 |
| **后台任务** | 基础实现 | 基于队列的限流机制 |
| **缓存预取** | 固定间隔 | 可配置批次 + 指数退避 |
| **统计指标** | 有限 | 前后台分离统计 |

## 相对上游项目的增强

本仓库是 [mokeyish/smartdns-rs](https://github.com/mokeyish/smartdns-rs) 的**生产硬化 fork**。在上游代码基础上，它增加了功能完整的 Web UI、一系列性能优化与健壮性修复——其中大部分源自**真实的长期运行部署问题**（在家庭局域网混合 UDP/DoT/DoH 上游环境下连续数周运行所暴露）。重点如下：

### Web UI（大幅扩展）

- **查询日志页面**（新增）：基于内存环形缓冲展示最近 1000 条查询，支持域名 / 客户端 / 记录类型过滤、自动刷新开关与手动刷新——由新增的 `/api/query-log` 端点支撑。
- **上游服务器统计**（新增）：每台上游的 IP、端口、协议（UDP/TCP/DoT/DoH/DoQ/DoH3）、安全性（加密/明文）、运行状态（正常/异常）、查询/成功/失败总数、平均响应耗时与成功率，以及聚合汇总——经由 `/api/nameservers` 与 `/api/stats/top-domains|clients|query-types` 提供。
- **总览页面**（新增）：热门域名与活跃客户端排行。
- **准确的缓存命中率**：以 O(1) 的逐查询命中计数器，取代此前"各条目命中数求和"的歧义算法（后者每次 `/stats` 轮询都要克隆整个缓存）。
- `web-ui` 特性**默认启用**——每次构建都自带仪表盘。

### 性能

- **Busy 快速失败**：上游返回 `Busy` 错误时不再重试（重试会占用信号量槽位并触发强制重连），切断了"busy → 重连 → 更慢 → 更多 busy"的恶性循环——此前长时间运行后上游 p95 延迟会被推高到约 440ms。
- **僵尸请求消除**：`NameServerGroup` 使用 detached task 替代带早期取消的 `FuturesUnordered`，竞速落败的查询在后台自然完成并释放 `DnsMultiplexer` 槽位，不再以"僵尸"形式累积耗尽 32 槽缓冲区。
- **Per-NameServer 并发限制**：每台上游独立的信号量，外加覆盖整个上游查询（含 TCP/TLS/QUIC 握手）的应用层截止时间/超时包裹。
- **预取管线修复**：基于堆的就绪队列（无全表扫描）、修复导致长期运行性能劣化的预取定时器泄漏、缓存分片锁、恢复否定缓存（NXDOMAIN/NODATA，含双栈 AAAA 未命中）——实测显著提升家庭网络下的缓存命中率。
- **中间件热路径**：削减 DNS 中间件链路上的冗余内存分配。

### 健壮性

- **每上游熔断**：连续失败计数与冷却期跳过；`SERVFAIL`/`REFUSED` 响应现在也会触发熔断。
- **panic 清扫**：约 20 处 `panic!`/`todo!()`/`expect()` 热路径已加固——空上游组、UDP 上游误配 HTTP 代理、监听器部分绑定失败、Mutex 中毒、非 UTF-8 命令行参数、未知配置指令、审计写线程失败、预取 `clear()` 漏清就绪堆等。
- **响应路径零阻塞**：审计写盘经由有界通道，绝不阻塞 DNS 响应；DoT 握手限时（slowloris 防护）；后台（预取）查询不再被静默丢弃，修复 `active_queries` 计数泄漏。

### 运维与 CI

- 每次版本发布都会将 Docker 镜像推送到 `ghcr.io`；可复现的发布管线（`workflow_dispatch` → GitHub Release，含 13 个目标平台的资产与 sha256sum）。
- 长期运维调优经验已沉淀进文档：`speed-check-mode none` 等推荐配置、缓存容量规划与上游健康排查（见下方故障排查）。

---

## 特性

- **多 DNS 上游服务器**

  支持配置多个上游 DNS 服务器，并同时进行查询，即使其中有 DNS 服务器异常，也不会影响查询。

- **返回最快 IP 地址**

  支持从域名所属 IP 地址列表中查找到访问速度最快的 IP 地址，并返回给客户端，提高网络访问速度。

- **支持多种查询协议**

  支持 UDP、TCP、DoT、DoQ、DoH 和 DoH3 查询及服务，以及非 53 端口查询；支持通过socks5，HTTP代理查询。

- **特定域名 IP 地址指定**

  支持指定域名的 IP 地址，达到广告过滤效果、避免恶意网站的效果。

- **域名分流**

  支持域名分流，不同类型的域名向不同的 DNS 服务器查询

- **Windows / MacOS / Linux 多平台支持**

  支持安装成服务开启自启动。

- **支持 IPv4、IPv6 双栈**

  支持 IPv4 和 IPV 6网络，支持查询 A 和 AAAA 记录，支持双栈 IP 速度优化，并支持完全禁用 IPv6 AAAA 解析。

- **支持DNS64**

  支持DNS64转换。

- **高性能、占用资源少**

  [Tokio](https://tokio.rs/) 加持的多线程异步 IO 模式；缓存查询结果；支持常用域名过期预读取，查询 **"0"** 毫秒，免除 DoH、DoT 加密带来的速度影响。

- **高级统计与监控**

  分别跟踪前台和后台查询指标，包括查询次数和平均响应时间。Web UI 实时显示综合平均查询时间、缓存命中耗时、上游查询耗时和后台预取耗时四项指标，便于精准定位性能瓶颈。

- **缓存预取优化**

  智能缓存预取功能支持可配置的批量限制（默认 5 个域名）、预取之间的最小间隔（默认 500ms），以及失败请求的指数退避机制，防止资源耗尽。

- **后台任务队列**

  基于有界通道的后台任务速率限制确保平滑的流量控制，防止工作线程饱和。

- **Web UI 仪表盘**

  内置 Web 仪表盘，采用单页面标签页界面，支持实时监控和管理。提供系统概览（含热门域名/活跃客户端排行）、上游服务器检查（每台上游的 IP/端口/协议/安全性/状态/查询次数/成功率/平均耗时）、查询日志浏览（支持过滤）、缓存管理（搜索/刷新）和规则管理（地址/转发 CRUD）。

*说明：C 语言版的 [smartdns](https://github.com/pymumu/smartdns) 功能非常的不错，但由于其仅支持 **Linux**，而对 **MacOS、Windows** 只能通过 Docker 或 WSL 支持。因此，才想开发一个 rust 版的 SmartDNS，支持编译到 Windows、MacOS、Linux 以及 Android 的 Termux 环境运行，并与其配置兼容。*

---

**目前仍在开发中，请勿用于生产环境，欢迎试用并提供反馈。**

请参考 [TODO](https://github.com/schalkiii/smartdns-rs/blob/main/TODO.md) 查看功能覆盖情况。

## 安装

_夜构建版本可在[这里](https://github.com/schalkiii/smartdns-rs/actions/workflows/nightly.yml)获取。_

- MacOS

  如果已安装 [brew](https://brew.sh/)，可直接使用以下命令安装。

  ```shell
  brew update
  brew install smartdns
  ```

  注意：监听 53 端口需要 root 权限，因此需要 `sudo`。

  对通过 brew 安装的 `smartdns`，命令 `sudo smartdns service start` 等价于 `sudo brew services start smartdns`。

  如果没有安装 `brew`，直接下载编译好的程序压缩包，按下面方式安装。

- Windows / Linux

  前往[这里](https://github.com/schalkiii/smartdns-rs/releases)下载压缩包并解压。
  1. 获取帮助

     ```shell
     ./smartdns --help
     ```

  2. 以前台方式运行，便于查看运行状态

     ```shell
     ./smartdns run -c ./smartdns.conf -v
     ```

     - `-v` 用于开启 debug 日志输出。

  3. 以后台服务方式运行，开机自启

     获取服务管理命令的帮助。

     ```shell
     ./smartdns service --help
     ```

     _注意：安装为系统服务需要管理员 / root 权限。_

     _服务管理兼容所有系统：Windows 上调用 [sc](<https://learn.microsoft.com/en-us/previous-versions/windows/it-pro/windows-server-2012-r2-and-2012/cc754599(v=ws.11)>)；MacOS 上调用 `launchctl` 或 `brew`；Linux 上调用 `Systemd` 或 `OpenRc`。_

## 配置

以下是最简单的示例配置

```conf
# 监听本地 53 端口
bind 127.0.0.1:53

# 配置 bootstrap-dns，若未配置则使用 system_conf，
# 建议配置，这样查询是加密的。
server https://1.1.1.1/dns-query  -bootstrap-dns -exclude-default-group
server https://8.8.8.8/dns-query  -bootstrap-dns -exclude-default-group

# 配置默认上游服务器
server https://cloudflare-dns.com/dns-query
server https://dns.quad9.net/dns-query
server https://dns.google/dns-query

# 配置办公室（家庭）上游服务器
server 192.168.1.1 -exclude-default-group -group office

# 以 ofc 结尾的域名将转发到 office 组解析
nameserver /ofc/office

# 为域名设置静态 IP
address /test.example.com/1.2.3.5

# 屏蔽域名（广告过滤）
address /ads.example.com/#

# 以下功能在 [C 语言版 SmartDNS](https://github.com/pymumu/smartdns) 中尚不支持，仅 SmartDNS-rs 适用。
# 配置 DoH3
server-h3 1.1.1.1

# 配置 DoQ
server-quic unfiltered.adguard-dns.com
```

更多高级配置请参考[这里](https://github.com/pymumu/smartdns/blob/doc/en/docs/configuration.md)，并参考 [TODO](https://github.com/schalkiii/smartdns-rs/blob/main/TODO.md) 查看功能覆盖情况。

## 内置 `dig` 诊断

SmartDNS-rs 支持内置的 `CHAOS TXT` 查询，用于服务器/客户端诊断。

```shell
# 最常用：完整身份信息（服务器 + 客户端，多条 TXT 记录）
dig @127.0.0.1 CH TXT whoami +short

# 仅服务器身份信息（多条 TXT 记录）
dig @127.0.0.1 CH TXT smartdns +short

# 服务器名称
dig @127.0.0.1 CH TXT server-name +short

# 服务器版本
dig @127.0.0.1 CH TXT version +short

# smartdns-rs 所见的客户端来源 IP
dig @127.0.0.1 CH TXT client_ip +short
dig @127.0.0.1 CH TXT client-ip +short

# 局域网内从 ARP 表获取的客户端 MAC（需 ARP 可用）
dig @127.0.0.1 CH TXT client_mac +short
dig @127.0.0.1 CH TXT client-mac +short

# JSON 输出（带后缀风格）
dig @127.0.0.1 CH TXT whoami.json +short
dig @127.0.0.1 CH TXT smartdns.json +short

# 兼容性示例
dig @127.0.0.1 CH TXT hostname.bind +short
dig @127.0.0.1 CH TXT version.bind +short
dig @127.0.0.1 CH TXT id.server +short
```

## Web UI 仪表盘

SmartDNS-rs 内嵌 Web 仪表盘，可通过 HTTP 访问。它以单页面标签页界面提供 DNS 查询统计、缓存检查、上游服务器状态与规则管理的实时监控。

### 启用仪表盘

1. **构建时启用 `web-ui` 特性**（默认已启用）：

   ```shell
   cargo build --release --features web-ui
   ```

   预编译的 Release 包与默认构建均包含该特性。

2. **在 `smartdns.conf` 中配置 HTTP 监听**：

   ```conf
   bind-http :8080
   ```

3. **启动 SmartDNS-rs**，然后在浏览器打开 `http://localhost:8080/dashboard`。

### 仪表盘分区

| 页签 | 说明 |
| --- | --- |
| **系统概览 (Overview)** | 运行时长、缓存命中率、平均查询耗时、总查询/活跃查询数、缓存条目数、查询趋势面积图、缓存条目排行、热门域名与活跃客户端排行。 |
| **上游服务器 (Upstream)** | 已配置的上游 DNS 服务器，含 IP、端口、协议（UDP/TCP/DoT/DoH/DoQ/DoH3）、安全性（加密/明文）、运行状态、查询/成功/失败次数、平均响应耗时与成功率，以及聚合汇总；监听端口配置。 |
| **缓存管理 (Cache)** | 缓存容量上限、当前条目数、可搜索的缓存条目表（含命中次数与最近访问时间）、一键清空缓存。 |
| **规则管理 (Rules)** | 地址规则（域名 → IP 映射）与转发规则管理，支持对话框新建与删除。 |
| **查询日志 (Query Log)** | 最近 1000 条查询（内存环形缓冲，重启后清空），支持域名/客户端/记录类型过滤、自动刷新开关与手动刷新。 |

### 关键指标

- **缓存命中率**：由 DNS 中间件中的逐查询计数器以 `query_hits / total_queries` 计算，提供准确的实时命中率跟踪。
- **查询趋势**：展示总查询数与缓存命中数随时间变化的面积图（最多 120 个快照）。
- **查询统计**：总查询数、活跃查询数、平均查询耗时与缓存条目数。

## 构建

假设你已经安装了 [Rust](https://www.rust-lang.org/learn/get-started)，打开终端执行以下命令：

```shell
git clone https://github.com/schalkiii/smartdns-rs.git
cd smartdns-rs

# 安装 https://github.com/casey/just
cargo install just

# 构建（默认包含 web-ui 仪表盘）
just build --release

# 构建（不含 web-ui 的最小二进制）
just build --release --no-default-features

# 打印帮助
./target/release/smartdns --help

# 运行
sudo ./target/release/smartdns run -c ./etc/smartdns/smartdns.conf
```

交叉编译推荐使用 [cross](https://github.com/cross-rs/cross)（需要 Docker）。

## 故障排查

### 长时间运行后网络异常（resource too busy）

**症状**：长时间运行后，DNS 查询变慢或失败，日志中频繁出现 `resource too busy` 错误。

**根因**：`NameServerGroup` 并行查询多个上游服务器时，首个成功响应会取消其余查询。这些被取消的查询在 `DnsMultiplexer` 中遗留"僵尸"请求条目，直到上游响应或超时才释放槽位。慢速上游（5s 超时）使僵尸累积并耗尽 32 个槽位缓冲区，导致新查询触发 `Busy` 错误。

**修复（已应用）**：`NameServerGroup` 现在使用 detached task 替代 `FuturesUnordered` 的早期返回。失去竞争的查询在后台自然完成并释放槽位，不再产生僵尸。

**建议的配置调优**：
- 从配置中移除不可靠的上游服务器（证书过期、频繁超时）——它们是僵尸累积和真实查询失败的主要来源。
- 若不需要基于 ping 的 IP 选择，使用 `speed-check-mode none` 以减少单次查询开销。
- 保持较大的 `cache-size`（如 65536）以最大化缓存命中率，降低上游负载。

### 上游 DNS 服务器健康

若日志中出现 `Failed to connect to any nameserver` 错误，请检查所配置上游服务器的健康状况：
- **证书过期**：移除或更新服务器 URL。
- **TLS 握手超时**：服务器可能不可达或过载，考虑移除。
- **请求超时**：网络路径问题。尝试更换服务器或协议（如从 DoH 切换为普通 UDP）。

使用 `log-level debug` 可查看详细的重试与错误信息。

## 致谢 !!!

本软件的诞生离不开：

- [Hickory DNS](https://github.com/hickory-dns/hickory-dns)
- [SmartDNS](https://github.com/pymumu/smartdns)

## 许可证

本软件包含来自 [https://github.com/hickory-dns/hickory-dns](https://github.com/hickory-dns/hickory-dns) 的代码，其采用以下任一许可证授权：

- Apache License, Version 2.0（LICENSE-APACHE 或 [http://www.apache.org/licenses/LICENSE-2.0](http://www.apache.org/licenses/LICENSE-2.0)）
- MIT 许可证（LICENSE-MIT 或 [http://opensource.org/licenses/MIT](http://opensource.org/licenses/MIT)）

其余代码采用以下许可证授权：

- GPL-3.0 许可证（LICENSE-GPL-3.0 或 [https://opensource.org/licenses/GPL-3.0](https://opensource.org/licenses/GPL-3.0)）

## 贡献

除非你另有明确声明，你有意提交用于纳入本作品的任何贡献，如 GPL-3.0 许可证所定义，均应按上述许可授权，且不附带任何额外的条款或条件。
