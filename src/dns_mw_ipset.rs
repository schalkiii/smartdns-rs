use std::net::IpAddr;

use crate::dns::*;
use crate::middleware::*;

/// `ipset /domain/<set>`：把应答中的 IP 下发给 ipset（Linux 通过 `ipset` CLI）。
pub struct DnsIpsetMiddleware;

#[async_trait::async_trait]
impl Middleware<DnsContext, DnsRequest, DnsResponse, DnsError> for DnsIpsetMiddleware {
    async fn handle(
        &self,
        ctx: &mut DnsContext,
        req: &DnsRequest,
        next: Next<'_, DnsContext, DnsRequest, DnsResponse, DnsError>,
    ) -> Result<DnsResponse, DnsError> {
        let res = next.run(ctx, req).await;

        if !ctx.server_opts.is_background
            && let (Ok(lookup), Some(rule)) = (res.as_ref(), &ctx.domain_rule)
        {
            let sets = rule.get(|r| r.ipsets.clone());
            let no_speed = ctx.cfg().ipset_no_speed().map(str::to_string);

            if sets.is_some() || no_speed.is_some() {
                let ip_addrs = lookup
                    .records()
                    .iter()
                    .filter_map(|r| r.data().ip_addr())
                    .collect::<Vec<_>>();

                if !ip_addrs.is_empty() {
                    // 对应 C 版 context.c:672：ipset-timeout 打开时取答案 TTL * 3
                    let timeout = if ctx.cfg().ipset_timeout_enable() {
                        lookup.max_ttl().unwrap_or(0) as u64 * 3
                    } else {
                        0
                    };

                    tokio::spawn(async move {
                        for set in sets.iter().flatten() {
                            for ip in &ip_addrs {
                                add(set, *ip, timeout);
                            }
                        }

                        #[cfg(target_os = "linux")]
                        if let Some(no_speed_set) = no_speed {
                            use crate::infra::ping::{PingAddr, PingOptions, ping_fastest};

                            for ip in ip_addrs {
                                let opts = PingOptions::default().with_timeout_secs(2);
                                if ping_fastest(vec![PingAddr::Icmp(ip)], opts).await.is_err() {
                                    add(&no_speed_set, ip, timeout);
                                }
                            }
                        }
                        #[cfg(not(target_os = "linux"))]
                        let _ = (no_speed, timeout);
                    });
                }
            }
        }

        res
    }
}

fn build_add_args(set: &str, ip: IpAddr, timeout: u64) -> Vec<String> {
    let mut args = vec![
        "add".to_string(),
        set.to_string(),
        ip.to_string(),
        "-exist".to_string(),
    ];

    if timeout > 0 {
        args.push("timeout".to_string());
        args.push(timeout.to_string());
    }

    args
}

#[cfg(target_os = "linux")]
fn add(set: &str, ip: IpAddr, timeout: u64) {
    let args = build_add_args(set, ip, timeout);
    match std::process::Command::new("ipset").args(&args).status() {
        Ok(status) if status.success() => {}
        Ok(status) => crate::log::debug!("ipset {} exited with {status}", args.join(" ")),
        Err(err) => crate::log::debug!("run ipset failed: {err}"),
    }
}

#[cfg(not(target_os = "linux"))]
fn add(_set: &str, _ip: IpAddr, _timeout: u64) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_add_args() {
        assert_eq!(
            build_add_args("myset", "1.2.3.4".parse().unwrap(), 0),
            vec!["add", "myset", "1.2.3.4", "-exist"]
        );
        assert_eq!(
            build_add_args("myset", "::1".parse().unwrap(), 90),
            vec!["add", "myset", "::1", "-exist", "timeout", "90"]
        );
    }
}
