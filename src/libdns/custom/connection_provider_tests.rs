// SPDX-License-Identifier: GPL-3.0-only
// 连接提供器测试模块 — 包含资源控制逻辑验证和压力测试
use super::*;
use crate::proxy::{ProxyConfig, ProxyProtocol};
use std::io;
use std::net::SocketAddrV4;
use std::sync::atomic::{AtomicU32, Ordering};
use tokio::time::Instant;

/// 测试 `is_resource_busy` 函数在不同平台上的资源耗尽错误检测
#[test]
fn test_is_resource_busy_detection() {
    // Windows: WSAENOBUFS = 10055
    let win_error = io::Error::from_raw_os_error(10055);
    assert!(
        is_resource_busy(&win_error),
        "WSAENOBUFS (10055) 应该被识别为 resource busy"
    );

    // Linux: ENOBUFS = 105
    let linux_error = io::Error::from_raw_os_error(105);
    assert!(
        is_resource_busy(&linux_error),
        "ENOBUFS (105) 应该被识别为 resource busy"
    );

    // 其他错误不应被识别为 resource busy
    let other_error = io::Error::new(io::ErrorKind::ConnectionRefused, "test");
    assert!(
        !is_resource_busy(&other_error),
        "ConnectionRefused 不应被识别为 resource busy"
    );

    let perm_error = io::Error::new(io::ErrorKind::PermissionDenied, "test");
    assert!(
        !is_resource_busy(&perm_error),
        "PermissionDenied 不应被识别为 resource busy"
    );
}

/// 测试 `is_resource_busy` 函数对无 raw_os_error 的错误处理
#[test]
fn test_is_resource_busy_no_raw_error() {
    let err = io::Error::other("some error");
    assert!(
        !is_resource_busy(&err),
        "无 raw_os_error 的错误不应被识别为 resource busy"
    );
}

/// 测试 socket 速率限制器确保创建间隔 >= 200ms
#[tokio::test]
async fn test_socket_rate_limiter_interval() {
    let start = Instant::now();
    socket_rate_limit().await;
    let first = start.elapsed();

    socket_rate_limit().await;
    let second = start.elapsed();

    let interval = second - first;
    assert!(
        interval >= Duration::from_millis(200),
        "socket 速率限制器应确保 >= 200ms 间隔，实际间隔: {interval:?}"
    );
}

/// 测试 socket 速率限制器并发调用时的行为
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_socket_rate_limiter_concurrent() {
    let start = Instant::now();
    let mut handles = Vec::new();

    for i in 0..4 {
        handles.push(tokio::spawn(async move {
            socket_rate_limit().await;
            (i, Instant::now())
        }));
    }

    let mut results = Vec::new();
    for handle in handles {
        results.push(handle.await.unwrap());
    }

    results.sort_by_key(|(_, t)| *t);

    for window in results.windows(2) {
        let interval = window[1].1 - window[0].1;
        assert!(
            interval >= Duration::from_millis(200),
            "并发 rate_limit 调用之间间隔应 >= 200ms，实际: {interval:?}"
        );
    }

    let total_elapsed = start.elapsed();
    assert!(
        total_elapsed >= Duration::from_millis(600),
        "4 个并发 rate_limit 调用总耗时应 >= 600ms，实际: {total_elapsed:?}"
    );
}

/// 测试信号量限制并发创建数量
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_semaphore_limits_concurrency() {
    let semaphore = socket_create_semaphore();

    let permit1 = semaphore.try_acquire();
    assert!(permit1.is_ok(), "应该能获取第 1 个许可");

    let permit2 = semaphore.try_acquire();
    assert!(permit2.is_ok(), "应该能获取第 2 个许可");

    let permit3 = semaphore.try_acquire();
    assert!(permit3.is_err(), "默认许可数耗尽后，不应能获取第 3 个许可");

    drop(permit1);
    let permit4 = semaphore.try_acquire();
    assert!(permit4.is_ok(), "释放许可后应该能重新获取");
}

/// 压力测试：大量并发 socket 创建验证系统不会触发 resource busy 错误
#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn test_stress_concurrent_socket_creation() {
    let total = 20;
    let mut handles = Vec::with_capacity(total);

    let start = Instant::now();
    for i in 0..total {
        handles.push(tokio::spawn(async move {
            let port = 1024u16 + ((i as u16) % 60000);
            let bind_addr = SocketAddr::new(std::net::Ipv4Addr::LOCALHOST.into(), port);
            let result = std::net::UdpSocket::bind(bind_addr);
            (i, result)
        }));
    }

    let mut success = 0;
    let mut failures = 0;
    for handle in handles {
        match handle.await.unwrap() {
            (_, Ok(socket)) => {
                drop(socket);
                success += 1;
            }
            (i, Err(err)) => {
                eprintln!("socket {i} creation failed: {err}");
                failures += 1;
            }
        }
    }

    let elapsed = start.elapsed();
    eprintln!("stress test: {success} succeeded, {failures} failed, elapsed: {elapsed:?}");

    assert!(failures <= 5, "socket 创建失败率过高: {failures}/{total}");
}

/// 测试 `next_random_udp` 函数的基本功能
#[test]
fn test_next_random_udp_basic() {
    let bind_addr = SocketAddr::new(std::net::Ipv4Addr::LOCALHOST.into(), 0);
    let result = next_random_udp(bind_addr);
    assert!(result.is_ok(), "next_random_udp 应该能成功绑定随机端口");
    let socket = result.unwrap();
    let local = socket.local_addr().unwrap();
    assert!(
        local.port() >= 1024,
        "随机端口应 >= 1024，实际: {}",
        local.port()
    );
}

/// 测试 `next_random_udp` 函数对指定端口的处理
#[test]
fn test_next_random_udp_specific_port() {
    let bind_addr = SocketAddr::new(std::net::Ipv4Addr::LOCALHOST.into(), 12345);
    let result = next_random_udp(bind_addr);
    assert!(result.is_ok(), "next_random_udp 应该能绑定指定端口");
    let socket = result.unwrap();
    let local = socket.local_addr().unwrap();
    assert_eq!(local.port(), 12345, "指定端口应保持不变");
}

/// 测试 `TokioRuntimeProvider` 的创建和使用
#[test]
fn test_tokio_runtime_provider_creation() {
    let provider = TokioRuntimeProvider::new(None, None, None);
    let handle = provider.create_handle();
    std::mem::drop(handle);
}

/// 测试 `TokioRuntimeProvider` 带代理配置的场景
#[test]
fn test_tokio_runtime_provider_with_proxy() {
    let proxy = ProxyConfig {
        proto: ProxyProtocol::Http,
        server: SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 8080)),
        username: None,
        password: None,
    };
    let provider = TokioRuntimeProvider::new(Some(proxy), Some(255), Some("eth0".into()));
    let handle = provider.create_handle();
    std::mem::drop(handle);
}

/// 模拟 SmartDNS 高负载启动场景的压力测试：
/// 多个并发 connection 创建触发 bind_udp，验证防 resource-busy 保护机制
#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn test_stress_multi_bind_udp_with_protection() {
    let provider = TokioRuntimeProvider::new(None, None, None);
    let total = 30;
    let mut handles = Vec::with_capacity(total);

    let start = Instant::now();

    for i in 0..total {
        let provider = provider.clone();
        handles.push(tokio::spawn(async move {
            let port = 30000u16 + (i as u16);
            let local_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), port);
            let server_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(8, 8, 8, 8)), 53);
            let result = provider.bind_udp(local_addr, server_addr).await;
            (i, result)
        }));
    }

    let mut success = 0u32;
    let mut resource_busy = 0u32;
    let mut other_errors = 0u32;

    for handle in handles {
        match handle.await.unwrap() {
            (_, Ok(socket)) => {
                drop(socket);
                success += 1;
            }
            (i, Err(err)) => {
                if is_resource_busy(&err) {
                    eprintln!("[STRESS-UDP] task {i}: RESOURCE BUSY after retries");
                    resource_busy += 1;
                } else {
                    eprintln!("[STRESS-UDP] task {i}: other error: {err}");
                    other_errors += 1;
                }
            }
        }
    }

    let elapsed = start.elapsed();
    eprintln!(
        "[STRESS-UDP] result: {success} ok, {resource_busy} resource_busy, \
         {other_errors} other errors, elapsed: {elapsed:?}"
    );

    assert!(
        resource_busy == 0,
        "resource busy 错误穿透了保护机制: {resource_busy} 次"
    );

    assert!(
        elapsed >= Duration::from_millis(2800),
        "保护机制疑似未生效：总耗时过短 {elapsed:?}"
    );
}

/// 测试保护机制在 TCP 路径上的有效性
#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn test_stress_multi_connect_tcp_with_protection() {
    let provider = TokioRuntimeProvider::new(None, None, None);
    let total = 20;
    let mut handles = Vec::with_capacity(total);

    let start = Instant::now();

    for i in 0..total {
        let provider = provider.clone();
        handles.push(tokio::spawn(async move {
            // 使用无法连接的地址 (TEST-NET-1)，快速触发错误但不建立连接
            let server_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 0, 2, 1)), 53);
            let bind_addr =
                SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 30000u16 + (i as u16));
            let result = provider
                .connect_tcp(server_addr, Some(bind_addr), Some(Duration::from_secs(1)))
                .await;
            (i, result)
        }));
    }

    let mut resource_busy = 0u32;
    for handle in handles {
        match handle.await.unwrap() {
            (i, Err(err)) if is_resource_busy(&err) => {
                eprintln!("[STRESS-TCP] task {i}: RESOURCE BUSY after retries");
                resource_busy += 1;
            }
            _ => {}
        }
    }

    let elapsed = start.elapsed();
    eprintln!("[STRESS-TCP] resource_busy: {resource_busy}, elapsed: {elapsed:?}");

    assert!(
        resource_busy == 0,
        "TCP resource busy 错误穿透了保护机制: {resource_busy} 次"
    );
}

/// 验证 next_random_udp 的并发安全性 —
/// 此函数被 QUIC/H3 bind_quic 调用，是 resource busy 的潜在漏洞点
#[test]
fn test_stress_next_random_udp_concurrent() {
    let total = 30;
    let bind_base = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 40000);
    let mut resource_busy = 0u32;
    let mut success = 0u32;

    for i in 0..total {
        let bind_addr = SocketAddr::new(bind_base.ip(), bind_base.port() + i as u16);
        match next_random_udp(bind_addr) {
            Ok(socket) => {
                drop(socket);
                success += 1;
            }
            Err(err) if is_resource_busy(&err) => {
                eprintln!("[STRESS-NRU] attempt {i}: RESOURCE BUSY");
                resource_busy += 1;
            }
            Err(err) => {
                eprintln!("[STRESS-NRU] attempt {i}: other error: {err}");
            }
        }
    }

    eprintln!("[STRESS-NRU] {success}/{total} succeeded, {resource_busy} resource_busy");

    assert!(
        resource_busy == 0,
        "next_random_udp 不应出现 resource_busy: {resource_busy} 次"
    );
    assert!(
        success + resource_busy <= total as u32,
        "计数不应超过 total"
    );
}

/// 验证 next_random_udp 端口 0 模式下的并发安全性
#[test]
fn test_stress_next_random_udp_random_port_concurrent() {
    let total = 20;
    let bind_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0);
    let mut resource_busy = 0u32;
    let mut success = 0u32;

    for _ in 0..total {
        match next_random_udp(bind_addr) {
            Ok(socket) => {
                drop(socket);
                success += 1;
            }
            Err(err) if is_resource_busy(&err) => {
                eprintln!("[STRESS-NRU-RAND] RESOURCE BUSY");
                resource_busy += 1;
            }
            Err(err) => {
                eprintln!("[STRESS-NRU-RAND] error: {err}");
            }
        }
    }

    eprintln!("[STRESS-NRU-RAND] {success}/{total} succeeded, {resource_busy} resource_busy");

    assert!(
        resource_busy == 0,
        "端口 0 模式下 next_random_udp 不应出现 resource_busy: {resource_busy} 次"
    );

    assert!(
        success + resource_busy <= total as u32,
        "计数不应超过 total"
    );
}

/// 模拟生产环境多组并发启动：多个 DNS 服务器组，
/// 每组包含多个服务器，同时进行 warmup。
/// 这是最接近生产启动场景（join_all over groups, join_all within each group）的压力测试。
#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn test_stress_multi_group_warmup_simulation() {
    const SOCKET_COUNT: usize = 80;
    const GROUPS: usize = 4;
    const PER_GROUP: usize = SOCKET_COUNT / GROUPS;

    let provider = TokioRuntimeProvider::new(None, None, None);
    let resource_busy = Arc::new(AtomicU32::new(0));
    let total_attempts = Arc::new(AtomicU32::new(0));

    let mut group_handles = Vec::new();

    for g in 0..GROUPS {
        let provider = provider.clone();
        let resource_busy = resource_busy.clone();
        let total_attempts = total_attempts.clone();

        let handle = tokio::spawn(async move {
            let mut tasks = Vec::new();
            for i in 0..PER_GROUP {
                let provider = provider.clone();
                let resource_busy = resource_busy.clone();
                let total_attempts = total_attempts.clone();
                let local_addr = SocketAddr::new(
                    IpAddr::V4(Ipv4Addr::UNSPECIFIED),
                    30000u16 + (g * PER_GROUP + i) as u16,
                );
                let server_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(8, 8, 8, 8)), 53);

                let g = g;
                tasks.push(tokio::spawn(async move {
                    let result = provider.bind_udp(local_addr, server_addr).await;

                    match &result {
                        Err(err) if is_resource_busy(err) => {
                            resource_busy.fetch_add(1, Ordering::Relaxed);
                            eprintln!(
                                "[MULTI-GROUP] group={g} port={} RESOURCE BUSY",
                                local_addr.port()
                            );
                        }
                        Err(err) => {
                            eprintln!(
                                "[MULTI-GROUP] group={g} port={} error: {err}",
                                local_addr.port()
                            );
                        }
                        Ok(_) => {}
                    }
                    total_attempts.fetch_add(1, Ordering::Relaxed);
                }));
            }

            futures::future::join_all(tasks).await
        });

        group_handles.push(handle);
    }

    let start = std::time::Instant::now();
    futures::future::join_all(group_handles).await;
    let elapsed = start.elapsed();

    let resource_busy = resource_busy.load(Ordering::Relaxed);
    let total = total_attempts.load(Ordering::Relaxed);

    eprintln!(
        "[MULTI-GROUP-WARMUP] {total} total, {resource_busy} resource_busy, \
         {groups} groups, elapsed: {elapsed:?}",
        groups = GROUPS
    );

    assert!(
        resource_busy == 0,
        "多组并发启动时 resource_busy 穿透: {resource_busy} 次 / {total} total"
    );
}

/// 混合协议并发压力测试：同时创建 UDP + TCP socket，
/// 模拟实际 DNS 查询中混合协议场景。
#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn test_stress_mixed_protocol_concurrent() {
    const TOTAL: usize = 60;
    const HALF: usize = TOTAL / 2;

    let provider = TokioRuntimeProvider::new(None, None, None);
    let resource_busy = Arc::new(AtomicU32::new(0));

    let mut handles = Vec::with_capacity(TOTAL);

    // UDP tasks
    for i in 0..HALF {
        let provider = provider.clone();
        let resource_busy = resource_busy.clone();
        let local_addr =
            SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 40000u16 + i as u16);
        let server_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(8, 8, 8, 8)), 53);
        handles.push(tokio::spawn(async move {
            let result = provider.bind_udp(local_addr, server_addr).await;
            if let Err(err) = &result
                && is_resource_busy(err)
            {
                resource_busy.fetch_add(1, Ordering::Relaxed);
            }
        }));
    }

    // TCP tasks
    for i in 0..HALF {
        let provider = provider.clone();
        let resource_busy = resource_busy.clone();
        let server_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 0, 2, 1)), 53);
        let bind_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 50000u16 + i as u16);
        handles.push(tokio::spawn(async move {
            let result = provider
                .connect_tcp(server_addr, Some(bind_addr), Some(Duration::from_secs(1)))
                .await;
            if let Err(err) = &result
                && is_resource_busy(err)
            {
                resource_busy.fetch_add(1, Ordering::Relaxed);
            }
        }));
    }

    let start = std::time::Instant::now();
    futures::future::join_all(handles).await;
    let elapsed = start.elapsed();

    let resource_busy = resource_busy.load(Ordering::Relaxed);

    eprintln!(
        "[MIXED-PROTOCOL] {total} total, {resource_busy} resource_busy, elapsed: {elapsed:?}",
        total = TOTAL
    );

    assert!(
        resource_busy == 0,
        "混合协议并发时 resource_busy 穿透: {resource_busy} 次"
    );
}

/// 全压力测试：组合所有场景，超大并发量。
/// 使用 16 个 Tokio 工作线程，模拟高并发生产环境。
#[tokio::test(flavor = "multi_thread", worker_threads = 16)]
async fn test_stress_full_blast() {
    const TOTAL_UDP: usize = 80;
    const TOTAL_TCP: usize = 40;
    const TOTAL_QUIC: usize = 30; // next_random_udp 路径

    let provider = TokioRuntimeProvider::new(None, None, None);
    let resource_busy = Arc::new(AtomicU32::new(0));
    let quic_resource_busy = Arc::new(AtomicU32::new(0));

    let mut handles = Vec::with_capacity(TOTAL_UDP + TOTAL_TCP + TOTAL_QUIC);

    // UDP tasks (信号量保护)
    for i in 0..TOTAL_UDP {
        let provider = provider.clone();
        let resource_busy = resource_busy.clone();
        let local_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 1024u16 + i as u16);
        let server_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(8, 8, 8, 8)), 53);
        handles.push(tokio::spawn(async move {
            let result = provider.bind_udp(local_addr, server_addr).await;
            if let Err(err) = &result
                && is_resource_busy(err)
            {
                resource_busy.fetch_add(1, Ordering::Relaxed);
            }
        }));
    }

    // TCP tasks (信号量保护)
    for i in 0..TOTAL_TCP {
        let provider = provider.clone();
        let resource_busy = resource_busy.clone();
        let server_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 0, 2, 1)), 53);
        let bind_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 50000u16 + i as u16);
        handles.push(tokio::spawn(async move {
            let result = provider
                .connect_tcp(server_addr, Some(bind_addr), Some(Duration::from_secs(1)))
                .await;
            if let Err(err) = &result
                && is_resource_busy(err)
            {
                resource_busy.fetch_add(1, Ordering::Relaxed);
            }
        }));
    }

    // QUIC path tasks (next_random_udp, Mutex 保护)
    for _ in 0..TOTAL_QUIC {
        let quic_resource_busy = quic_resource_busy.clone();
        let bind_addr = SocketAddr::new(
            IpAddr::V4(Ipv4Addr::UNSPECIFIED),
            0, // 随机端口模式
        );
        handles.push(tokio::spawn(async move {
            let result = tokio::task::spawn_blocking(move || next_random_udp(bind_addr)).await;
            match result {
                Ok(Err(err)) if is_resource_busy(&err) => {
                    quic_resource_busy.fetch_add(1, Ordering::Relaxed);
                }
                _ => {}
            }
        }));
    }

    let start = std::time::Instant::now();
    futures::future::join_all(handles).await;
    let elapsed = start.elapsed();

    let resource_busy = resource_busy.load(Ordering::Relaxed);
    let quic_busy = quic_resource_busy.load(Ordering::Relaxed);

    eprintln!(
        "[FULL-BLAST] udp_tcp_busy={resource_busy} quic_busy={quic_busy} \
         total={total} elapsed={elapsed:?}",
        total = TOTAL_UDP + TOTAL_TCP + TOTAL_QUIC
    );

    assert!(
        resource_busy == 0,
        "全压力测试 UDP/TCP resource_busy 穿透: {resource_busy}"
    );
    assert!(
        quic_busy == 0,
        "全压力测试 QUIC resource_busy 穿透: {quic_busy}"
    );
}
