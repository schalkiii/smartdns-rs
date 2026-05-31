# SmartDNS-rs

![Test](https://github.com/mokeyish/smartdns-rs/actions/workflows/test.yml/badge.svg?branch=main)
[![Crates.io Version](https://img.shields.io/crates/v/smartdns.svg)](https://crates.io/crates/smartdns)
[![GitHub release (latest by date including pre-releases)](https://img.shields.io/github/v/release/mokeyish/smartdns-rs?display_name=tag&include_prereleases)](https://github.com/mokeyish/smartdns-rs/releases)
[![homebrew version](https://img.shields.io/homebrew/v/smartdns)](https://formulae.brew.sh/formula/smartdns)
![OS](https://img.shields.io/badge/os-Windows%20%7C%20MacOS%20%7C%20Linux-blue)

[Docs](https://pymumu.github.io/smartdns/) •

[English](https://github.com/mokeyish/smartdns-rs/blob/main/README.md) | 中文

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

  分别跟踪前台和后台查询指标，包括查询次数和平均响应时间。Web UI 实时显示用户查询和后台缓存预取的统计数据。

- **缓存预取优化**

  智能缓存预取功能支持可配置的批量限制（默认 5 个域名）、预取之间的最小间隔（默认 500ms），以及失败请求的指数退避机制，防止资源耗尽。

- **后台任务队列**

  基于有界通道的后台任务速率限制确保平滑的流量控制，防止工作线程饱和。

- **Web UI 仪表盘**

  内置 Web 仪表盘，采用单页面标签页界面，支持实时监控和管理。提供系统概览、上游服务器检查、缓存管理（搜索/刷新）和规则管理（地址/转发 CRUD）。

*说明：C 语言版的 [smartdns](https://github.com/pymumu/smartdns) 功能非常的不错，但由于其仅支持 **Linux**，而对 **MacOS、Windows** 只能通过 Docker 或 WSL 支持。因此，才想开发一个 rust 版的 SmartDNS，支持编译到 Windows、MacOS、Linux 以及 Android 的 Termux 环境运行，并与其配置兼容。*

---

**目前仍在开发中，请勿用于生产环境，欢迎试用并提供反馈。**

请参考 [TODO](https://github.com/mokeyish/smartdns-rs/blob/main/TODO.md) 查看功能覆盖情况。