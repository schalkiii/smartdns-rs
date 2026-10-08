# SmartDNS-rs

![Test](https://github.com/schalkiii/smartdns-rs/actions/workflows/test.yml/badge.svg?branch=main)
[![GitHub release (latest by date including pre-releases)](https://img.shields.io/github/v/release/schalkiii/smartdns-rs?display_name=tag&include_prereleases)](https://github.com/schalkiii/smartdns-rs/releases)
![OS](https://img.shields.io/badge/os-Windows%20%7C%20MacOS%20%7C%20Linux-blue)

[Docs](https://pymumu.github.io/smartdns/en/) •

English | [中文](https://github.com/schalkiii/smartdns-rs/blob/main/README_zh-CN.md)

SmartDNS-rs 🐋 is a local DNS server imspired by [C SmartDNS](https://github.com/pymumu/smartdns) to accepts DNS query requests from local clients, obtains DNS query results from multiple upstream DNS servers, and returns the fastest access results to clients. Avoiding DNS pollution and improving network access speed, supports high-performance ad filtering.

---

## Why SmartDNS-rs?

This project is a **Rust reimplementation** of [SmartDNS](https://github.com/pymumu/smartdns) with several architectural and performance improvements:

| Feature               | SmartDNS (C)                            | SmartDNS-rs (Rust)                       |
| --------------------- | --------------------------------------- | ---------------------------------------- |
| **Platform Support**  | Linux only (with Docker/WSL for others) | Native Windows, macOS, Linux, Android    |
| **HTTP Service**      | Separate process required               | Built-in async HTTP server               |
| **Concurrency Model** | Thread-based                            | Tokio async runtime                      |
| **Memory Safety**     | Manual memory management                | Compile-time safety guarantees           |
| **Configuration**     | File-based only                         | File + REST API hot-reload               |
| **Web UI**            | Not included                            | Built-in dashboard                       |
| **Background Tasks**  | Basic                                   | Queue-based with rate limiting           |
| **Cache Prefetch**    | Fixed interval                          | Configurable batch + exponential backoff |
| **Statistics**        | Limited                                 | Separate foreground/background metrics   |

## Enhancements over the upstream project

This repository is a production-hardened fork of [mokeyish/smartdns-rs](https://github.com/mokeyish/smartdns-rs). Beyond the upstream codebase it adds a fully featured Web UI, a series of performance optimizations and robustness fixes — most of them driven by **real long-running deployment issues** (weeks of continuous operation in a home LAN with mixed UDP/DoT/DoH upstreams). Highlights:

### Web UI (major expansion)

- **Query Log page** (new): the most recent 1000 queries from an in-memory ring buffer, with domain / client / record-type filters, auto-refresh toggle and manual refresh — backed by the new `/api/query-log` endpoint.
- **Upstream server statistics** (new): per-server IP, port, protocol (UDP/TCP/DoT/DoH/DoQ/DoH3), security (encrypted/plaintext), runtime status (healthy/error), query/success/failure totals, average response time and success rate, plus aggregate totals — via `/api/nameservers` and `/api/stats/top-domains|clients|query-types`.
- **Overview page** (new): top domains & active clients ranking.
- **Accurate cache hit rate**: replaced the ambiguous "sum of per-entry hit counts" (which required cloning the entire cache on every `/stats` poll) with an O(1) per-query hit counter.
- The `web-ui` feature is **enabled by default** — every build ships with the dashboard.

### Performance

- **Busy fast-fail**: upstream `Busy` errors are no longer retried (which occupied semaphore slots and triggered forced reconnections), cutting the "busy → reconnect → slower → busier" death spiral that used to push p95 upstream latency to ~440ms after weeks of uptime.
- **Zombie request elimination**: `NameServerGroup` uses detached tasks instead of `FuturesUnordered` with early cancel, so race-losing queries finish naturally and release their `DnsMultiplexer` slots instead of accumulating as "zombies" that exhaust the 32-slot buffer.
- **Per-NameServer concurrency limiting**: a dedicated semaphore per upstream server, plus an application-level deadline/timeout wrapper covering the whole upstream lookup (including TCP/TLS/QUIC handshake).
- **Prefetch pipeline fixes**: heap-based ready queue (no full scans), prefetch timer leak fix that degraded long-running performance, cache-shard locking, and negative caching (NXDOMAIN/NODATA, including dual-stack AAAA misses) restored — measurably improving cache hit rate in home networks.
- **Middleware hot path**: reduced redundant allocations in the DNS middleware chain.

### Robustness

- **Circuit breaker per upstream**: consecutive-failure tracking with cooldown skipping; `SERVFAIL`/`REFUSED` responses now also trip the breaker.
- **Panic elimination sweep**: ~20 `panic!`/`todo!()`/`expect()` hot paths hardened — empty upstream groups, UDP upstreams misconfigured with an HTTP proxy, partial listener bind failures, poisoned mutexes, non-UTF-8 CLI args, unknown config directives, audit write-thread failures, prefetch `clear()` missing the ready heap, and more.
- **No blocking on the response path**: audit writes go through a bounded channel and never block DNS responses; DoT handshakes are time-bounded (slowloris protection); background (prefetch) queries are no longer silently dropped, fixing `active_queries` counter leaks.

### Operations & CI

- Docker images published to `ghcr.io` on every release; reproducible release pipeline (`workflow_dispatch` → GitHub Release with per-platform assets + sha256sums, 13 targets).
- Long-running operational tuning captured in the docs: recommended config for `speed-check-mode none`, cache sizing, and upstream health triage (see Troubleshooting below).

---

## Features

- **Multiple upstream DNS servers**

  Supports configuring multiple upstream DNS servers and query at the same time.the query will not be affected, Even if there is a DNS server exception.

- **Return the fastest IP address**

  Supports finding the fastest access IP address from the IP address list of the domain name and returning it to the client to avoid DNS pollution and improve network access speed.

- **Support for multiple query protocols**

  Supports UDP, TCP, DoT, DoQ, DoH, DoH3 queries and service, and non-53 port queries, effectively avoiding DNS pollution and protect privacy, and support query DNS over socks5, http proxy.

- **Domain IP address specification**

  Supports configuring IP address of specific domain to achieve the effect of advertising filtering, and avoid malicious websites.

- **DNS domain forwarding**

  Supports DNS forwarding, ipset and nftables. Support setting the domain result to ipset and nftset set when speed check fails.

- **Windows / MacOS / Linux multi-platform support**

  Supports installing as a service and running it at startup.

- **Support IPV4, IPV6 dual stack**

  Supports IPV4, IPV6 network, support query A, AAAA record, dual-stack IP selection, and filter IPV6 AAAA record.

- **DNS64**

  Supports DNS64 translation.

- **High performance, low resource consumption**

  Tokio-based multi-threaded asynchronous I/O model; caches query results; supports most-used domain name expired prefetching, query **'0'** milliseconds, without eliminating the impact of DoH and DoT encryption.

- **Advanced Statistics & Monitoring**

  Separately tracks foreground and background query metrics including query counts and average response times. The Web UI displays four real-time metrics — overall average query time, cache-hit query time, upstream query time, and background prefetch time — making it easy to pinpoint performance bottlenecks.

- **Cache Prefetch Optimization**

  Intelligent cache prefetching with configurable batch limits (default 5 domains), minimum interval between prefetches (default 500ms), and exponential backoff for failed requests to prevent resource exhaustion.

- **Background Task Queue**

  Bounded channel-based rate limiting for background tasks ensures smooth traffic control and prevents worker thread saturation.

- **Web UI Dashboard**

  Built-in web dashboard with a single-page tabbed interface for real-time monitoring and management. Supports system overview (with top domains/clients ranking), upstream server inspection (per-server IP/port/protocol/security/status/query counts/success rate/average time), query log browsing with filters, cache management with search/flush, and rule management with address/forward CRUD.

Note: The C version of smartdns is very functional, but because it only supports **Linux**, while **MacOS and Windows** can only be supported through Docker or WSL. Therefore, I want to develop a rust version of SmartDNS that supports compiling to Windows, MacOS, Linux and Android Termux environment to run, and is compatible with its configuration.

---

**It is still under development, please do not use it in production environment, welcome to try and provide feedback.**

Please refer to [TODO](https://github.com/schalkiii/smartdns-rs/blob/main/TODO.md) for the function coverage

## Installing

_Nightly builds can be found [here](https://github.com/schalkiii/smartdns-rs/actions/workflows/nightly.yml)._

- MacOS

  If you have installed [brew](https://brew.sh/), you can directly use the following command to install.

  ```shell
  brew update
  brew install smartdns
  ```

  Note: Listening on port 53 requires root permission, so `sudo` is required.

  The command `sudo smartdns service start` for `brew` installed `smartdns` is the same as `sudo brew services start smartdns`.

  If you don't have `brew` installed, just download the compiled program compression package and install it as below.

- Windows / Linux

  Go to [here](https://github.com/schalkiii/smartdns-rs/releases) to download the package and decompress it.
  1. Get help

     ```shell
     ./smartdns --help
     ```

  2. Run as foreground, easy to check the running status

     ```shell
     ./smartdns run -c ./smartdns.conf -v
     ```

     - `-v` is enabled to print debug logs.

  3. Run as background service, run automatically at startup

     Get help of service management commands.

     ```shell
     ./smartdns service --help
     ```

     _Note: Installed as a system service, administrator / root permissions are required._

     _Service management is compatible with all systems, call [sc](<https://learn.microsoft.com/en-us/previous-versions/windows/it-pro/windows-server-2012-r2-and-2012/cc754599(v=ws.11)>) on Windows; call `launchctl` or `brew` on MacOS; call `Systemd` or `OpenRc` on Linux._

## Configuration

The following is the simplest example configuration

```conf
# Listen on local port 53
bind 127.0.0.1:53

# Configure bootstrap-dns, if not configured, call the system_conf,
# it is recommended to configure, so that it will be encrypted.
server https://1.1.1.1/dns-query  -bootstrap-dns -exclude-default-group
server https://8.8.8.8/dns-query  -bootstrap-dns -exclude-default-group

# Configure default upstream server
server https://cloudflare-dns.com/dns-query
server https://dns.quad9.net/dns-query
server https://dns.google/dns-query

# Configure the Office(Home) upstream server
server 192.168.1.1 -exclude-default-group -group office

# Domain names ending with ofc are forwarded to the office group for resolution
nameserver /ofc/office

# Set static IP for domain name
address /test.example.com/1.2.3.5

# Block Domains (Ad Blocking)
address /ads.example.com/#

# The following features are not yet supported in the [C SmartDNS](https://github.com/pymumu/smartdns) and are only applicable to SmartDNS-rs.
# Configure DoH3
server-h3 1.1.1.1

# Configure DoQ
server-quic unfiltered.adguard-dns.com
```

For more advanced configurations, please refer to [here](https://github.com/pymumu/smartdns/blob/doc/en/docs/configuration.md) , and refer to [TODO](https://github.com/schalkiii/smartdns-rs/blob/main/TODO.md) for the function coverage.

## Built-in diagnostics via `dig`

SmartDNS-rs supports built-in `CHAOS TXT` queries for server/client diagnostics.

```shell
# most common: full identity info (server + client, multi TXT records)
dig @127.0.0.1 CH TXT whoami +short

# server identity info only (multi TXT records)
dig @127.0.0.1 CH TXT smartdns +short

# server name
dig @127.0.0.1 CH TXT server-name +short

# server version
dig @127.0.0.1 CH TXT version +short

# client source IP seen by smartdns-rs
dig @127.0.0.1 CH TXT client_ip +short
dig @127.0.0.1 CH TXT client-ip +short

# client MAC from ARP table (LAN, ARP available)
dig @127.0.0.1 CH TXT client_mac +short
dig @127.0.0.1 CH TXT client-mac +short

# JSON output with suffix style
dig @127.0.0.1 CH TXT whoami.json +short
dig @127.0.0.1 CH TXT smartdns.json +short

# Compatibility examples
dig @127.0.0.1 CH TXT hostname.bind +short
dig @127.0.0.1 CH TXT version.bind +short
dig @127.0.0.1 CH TXT id.server +short
```

## Web UI Dashboard

SmartDNS-rs includes an embedded web dashboard accessible over HTTP. It provides real-time monitoring of DNS query statistics, cache inspection, upstream server status, and rule management — all in a single-page tabbed interface.

### Enabling the Dashboard

1. **Enable the `web-ui` feature at build time** (already enabled by default):

   ```shell
   cargo build --release --features web-ui
   ```

   Pre-built releases and default builds include this feature.

2. **Configure the HTTP listener** in your `smartdns.conf`:

   ```conf
   bind-http :8080
   ```

3. **Start SmartDNS-rs** and open `http://localhost:8080/dashboard` in your browser.

### Dashboard Sections

| Tab                       | Description                                                                                                                            |
| ------------------------- | -------------------------------------------------------------------------------------------------------------------------------------- |
| **系统概览 (Overview)**   | Uptime, cache hit rate, average query time, total/active queries, cache entry count, query trend area chart, top cache entries, top domains & active clients ranking.        |
| **上游服务器 (Upstream)** | Configured upstream DNS servers with IP, port, protocol (UDP/TCP/DoT/DoH/DoQ/DoH3), security (encrypted/plaintext), runtime status, query/success/failure counts, average response time and success rate, plus aggregate totals. Listener port configuration. |
| **缓存管理 (Cache)**      | Cache size limit, current entry count, searchable cache entry table with hit counts and last access timestamps, one-click cache flush. |
| **规则管理 (Rules)**      | Address rules (domain → IP mapping) and forward rules management. Supports creating new rules via dialog and deleting existing ones.   |
| **查询日志 (Query Log)**  | Most recent 1000 queries (in-memory ring buffer, cleared on restart) with domain/client/record-type filters, auto-refresh toggle and manual refresh. |

### Key Metrics

- **Cache hit rate**: calculated as `query_hits / total_queries` using a per-query counter in the DNS middleware, providing accurate real-time hit rate tracking.
- **Query trend**: area chart showing total queries vs cache hits over time (up to 120 snapshots).
- **Query statistics**: total queries, active queries, average query time, and cache entry count.

## Building

Assuming you have installed [Rust](https://www.rust-lang.org/learn/get-started), then you can open the terminal and execute these commands:

```shell
git clone https://github.com/schalkiii/smartdns-rs.git
cd smartdns-rs

# install https://github.com/casey/just
cargo install just

# build (web-ui dashboard included by default)
just build --release

# build (without web-ui, minimal binary)
just build --release --no-default-features

# print help
./target/release/smartdns --help

# run
sudo ./target/release/smartdns run -c ./etc/smartdns/smartdns.conf
```

For cross-compilation, it is recommended to use [cross](https://github.com/cross-rs/cross) (requires Docker).

## Troubleshooting

### Long-running network anomalies (resource too busy)

**Symptom**: After running for a long time, DNS queries become slow or fail, and logs show frequent `resource too busy` errors.

**Root cause**: When `NameServerGroup` queries multiple upstream servers in parallel, the first successful response cancels the remaining queries. These cancelled queries leave "zombie" request entries in the `DnsMultiplexer` until the upstream server responds or times out. Slow upstream servers (5s timeout) allow zombies to accumulate and exhaust the 32-slot buffer, causing new queries to fail with `Busy` errors.

**Fix (already applied)**: NameServerGroup now uses detached tasks instead of `FuturesUnordered` with early return. Queries that lose the race complete naturally in the background, releasing their slots without creating zombies.

**Recommended config tuning**:
- Remove unreliable upstream servers (expired certs, frequent timeouts) from your config — they are the primary source of both zombie accumulation and real query failures.
- Use `speed-check-mode none` if you don't need ping-based IP selection, to reduce per-query overhead.
- Keep `cache-size` large (e.g., 65536) to maximize cache hit rate and reduce upstream load.

### Upstream DNS server health

If you see `Failed to connect to any nameserver` errors in logs, check the health of your configured upstream servers:
- **Certificate expired**: Remove or update the server URL.
- **TLS handshake timeout**: The server may be unreachable or overloaded. Consider removing it.
- **Request timeout**: Network path issues. Try a different server or protocol (e.g., switch from DoH to plain UDP).

Use `log-level debug` to see detailed retry and error information.

## Acknowledgments !!!

This software wouldn't have been possible without:

- [Hickory DNS](https://github.com/hickory-dns/hickory-dns)
- [SmartDNS](https://github.com/pymumu/smartdns)

## License

This software contains codes from [https://github.com/hickory-dns/hickory-dns](https://github.com/hickory-dns/hickory-dns), which is licensed under either of

- Apache License, Version 2.0, (LICENSE-APACHE or [http://www.apache.org/licenses/LICENSE-2.0](http://www.apache.org/licenses/LICENSE-2.0))
- MIT license (LICENSE-MIT or [http://opensource.org/licenses/MIT](http://opensource.org/licenses/MIT))

And other codes is licensed under

- GPL-3.0 license (LICENSE-GPL-3.0 or [https://opensource.org/licenses/GPL-3.0](https://opensource.org/licenses/GPL-3.0))

## Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you, as defined in the GPL-3.0 license, shall be licensed as above, without any additional terms or conditions.
