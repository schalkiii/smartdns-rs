use std::net::IpAddr;

use crate::config::{ConfigForIP, NFTsetConfig};
use crate::dns::*;
use crate::ffi::nftset;
use crate::log;
use crate::middleware::*;

pub struct DnsNftsetMiddleware;

#[async_trait::async_trait]
impl Middleware<DnsContext, DnsRequest, DnsResponse, DnsError> for DnsNftsetMiddleware {
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
            let nftsets = rule.get(|n| n.nftset.as_ref().cloned());
            if let Some(nftsets) = nftsets {
                let ip_addrs = lookup
                    .records()
                    .iter()
                    .filter_map(|r| r.data().ip_addr())
                    .collect::<Vec<_>>();

                if !ip_addrs.is_empty() {
                    // 对应 C 版 context.c:672：开启 nftset-timeout 时元素超时取答案 TTL * 3
                    let timeout = if ctx.cfg().nftset_timeout_enable() {
                        lookup.max_ttl().unwrap_or(0) as u64 * 3
                    } else {
                        0
                    };
                    let debug = ctx.cfg().nftset_debug();

                    tokio::spawn(async move {
                        add_addrs(&nftsets, &ip_addrs, timeout, debug);
                    });
                }
            }
        }
        res
    }
}

fn add_addrs(
    nftsets: &[ConfigForIP<NFTsetConfig>],
    ip_addrs: &[IpAddr],
    timeout: u64,
    debug: bool,
) {
    for nftset in nftsets {
        let (family, table, name) = match nftset {
            ConfigForIP::V4(cfg) => (cfg.family, cfg.table.as_str(), cfg.name.as_str()),
            ConfigForIP::V6(cfg) => (cfg.family, cfg.table.as_str(), cfg.name.as_str()),
            ConfigForIP::None => continue,
        };
        let want_v4 = matches!(nftset, ConfigForIP::V4(_));

        for ip in ip_addrs.iter().filter(|ip| ip.is_ipv4() == want_v4) {
            if debug {
                log::debug!("[nftset] add {ip} to {family}#{table}#{name} (timeout {timeout})");
            }
            let _ = nftset::add(family, table, name, *ip, timeout);
        }
    }
}
