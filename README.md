# SmartDNS-rs

![Test](https://github.com/mokeyish/smartdns-rs/actions/workflows/test.yml/badge.svg?branch=main)
[![Crates.io Version](https://img.shields.io/crates/v/smartdns.svg)](https://crates.io/crates/smartdns)
[![GitHub release (latest by date including pre-releases)](https://img.shields.io/github/v/release/mokeyish/smartdns-rs?display_name=tag&include_prereleases)](https://github.com/mokeyish/smartdns-rs/releases)
[![homebrew version](https://img.shields.io/homebrew/v/smartdns)](https://formulae.brew.sh/formula/smartdns)
![OS](https://img.shields.io/badge/os-Windows%20%7C%20MacOS%20%7C%20Linux-blue)

[Docs](https://pymumu.github.io/smartdns/en/) •

English | [中文](https://github.com/mokeyish/smartdns-rs/blob/main/README_zh-CN.md)

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

  Separately tracks foreground and background query metrics including query counts and average response times. Web UI displays real-time statistics for both user-facing queries and background cache prefetch operations.

- **Cache Prefetch Optimization**

  Intelligent cache prefetching with configurable batch limits (default 5 domains), minimum interval between prefetches (default 500ms), and exponential backoff for failed requests to prevent resource exhaustion.

- **Background Task Queue**

  Bounded channel-based rate limiting for background tasks ensures smooth traffic control and prevents worker thread saturation.

- **Web UI Dashboard**

  Built-in web dashboard with a single-page tabbed interface for real-time monitoring and management. Supports system overview, upstream server inspection, cache management with search/flush, and rule management with address/forward CRUD.

Note: The C version of smartdns is very functional, but because it only supports **Linux**, while **MacOS and Windows** can only be supported through Docker or WSL. Therefore, I want to develop a rust version of SmartDNS that supports compiling to Windows, MacOS, Linux and Android Termux environment to run, and is compatible with its configuration.

---

**It is still under development, please do not use it in production environment, welcome to try and provide feedback.**

Please refer to [TODO](https://github.com/mokeyish/smartdns-rs/blob/main/TODO.md) for the function coverage

## Installing

_Nightly builds can be found [here](https://github.com/mokeyish/smartdns-rs/actions/workflows/nightly.yml)._

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

  Go to [here](https://github.com/mokeyish/smartdns-rs/releases) to download the package and decompress it.
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

For more advanced configurations, please refer to [here](https://github.com/pymumu/smartdns/blob/doc/en/docs/configuration.md) , and refer to [TODO](https://github.com/mokeyish/smartdns-rs/blob/main/TODO.md) for the function coverage.

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

1. **Enable the `web-ui` feature at build time:**

   ```shell
   cargo build --release --features web-ui
   ```

   Pre-built releases include this feature by default.

2. **Configure the HTTP listener** in your `smartdns.conf`:

   ```conf
   bind-http :8080
   ```

3. **Start SmartDNS-rs** and open `http://localhost:8080/dashboard` in your browser.

### Dashboard Sections

| Tab                       | Description                                                                                                                            |
| ------------------------- | -------------------------------------------------------------------------------------------------------------------------------------- |
| **系统概览 (Overview)**   | Uptime, cache hit rate, average query time, total/active queries, cache entry count, query trend area chart, top cache entries.        |
| **上游服务器 (Upstream)** | List of configured upstream DNS servers with protocol labels, listener port configuration.                                             |
| **缓存管理 (Cache)**      | Cache size limit, current entry count, searchable cache entry table with hit counts and last access timestamps, one-click cache flush. |
| **规则管理 (Rules)**      | Address rules (domain → IP mapping) and forward rules management. Supports creating new rules via dialog and deleting existing ones.   |

### Key Metrics

- **Cache hit rate**: calculated as `query_hits / total_queries` using a per-query counter in the DNS middleware, providing accurate real-time hit rate tracking.
- **Query trend**: area chart showing total queries vs cache hits over time (up to 120 snapshots).
- **Query statistics**: total queries, active queries, average query time, and cache entry count.

## Building

Assuming you have installed [Rust](https://www.rust-lang.org/learn/get-started), then you can open the terminal and execute these commands:

```shell
git clone https://github.com/mokeyish/smartdns-rs.git
cd smartdns-rs

# install https://github.com/casey/just
cargo install just

# build
just build --release

# print help
./target/release/smartdns --help

# run
sudo ./target/release/smartdns run -c ./etc/smartdns/smartdns.conf
```

For cross-compilation, it is recommended to use [cross](https://github.com/cross-rs/cross) (requires Docker).

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
