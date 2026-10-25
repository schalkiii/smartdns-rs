use std::ops::Deref;

use crate::libdns::proto::op::ResponseCode;

use crate::dns::*;

use crate::middleware::*;

pub struct DnsBogusMiddleware;

#[async_trait::async_trait]
impl Middleware<DnsContext, DnsRequest, DnsResponse, DnsError> for DnsBogusMiddleware {
    async fn handle(
        &self,
        ctx: &mut DnsContext,
        req: &DnsRequest,
        next: Next<'_, DnsContext, DnsRequest, DnsResponse, DnsError>,
    ) -> Result<DnsResponse, DnsError> {
        let res = next.run(ctx, req).await;

        let bogus_nxdomain = ctx.cfg().bogus_nxdomain();
        // ip-rules 中按客户端 CIDR 生效的 -bogus-nxdomain
        let client_bogus = if ctx.cfg().ip_rules().is_empty() {
            None
        } else {
            Some(ctx.cfg().ip_rule_bogus_set_for(req.src().ip()))
        };

        if req.query().query_type().is_ip_addr()
            && let Ok(lookup) = res.as_ref()
        {
            for record in lookup.records() {
                let is_bogus = |ip: &std::net::IpAddr| {
                    bogus_nxdomain.contains(ip)
                        || client_bogus.as_ref().is_some_and(|set| set.contains(ip))
                };

                if match record.data() {
                    RData::A(ip) if is_bogus(&std::net::IpAddr::V4(*ip.deref())) => true,
                    RData::AAAA(ip) if is_bogus(&std::net::IpAddr::V6(*ip.deref())) => true,
                    _ => false,
                } {
                    return Err(ResponseCode::NXDomain.into());
                }
            }
        }
        res
    }
}
