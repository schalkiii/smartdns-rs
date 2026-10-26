use std::borrow::Cow;
use std::time::{Duration, Instant};

use crate::dns::*;
use crate::libdns::proto::rr::{RData, RecordType};
use crate::middleware::*;

use crate::libdns::resolver::TtlClip;

#[derive(Debug)]
pub struct AddressMiddleware;

/// `rr-ttl` / `rr-ttl-min` / `rr-ttl-max` 的生效边界：域规则优先于全局。
///
/// C 版在缓存写入（`cache.c:140`）与回复构建（`context.c:807` 经 `rules.c:397`）
/// 都用同一套边界，因此这里同时供 address 中间件（回复）与 cache 中间件（入库）使用。
pub(crate) fn rr_ttl_bounds(ctx: &DnsContext) -> (Option<u32>, Option<u32>) {
    let min = ctx
        .domain_rule
        .get_ref(|r| r.rr_ttl_min.as_ref())
        .cloned()
        .or_else(|| ctx.cfg().rr_ttl_min())
        .map(|i| i as u32);

    let max = ctx
        .domain_rule
        .get_ref(|r| r.rr_ttl_max.as_ref())
        .cloned()
        .or_else(|| ctx.cfg().rr_ttl_max())
        .map(|i| i as u32);

    (min, max)
}

/// 按边界夹取记录 TTL（先 min 后 max，与 C 的 `_dns_server_get_conf_ttl` 顺序一致）。
pub(crate) fn clamp_ttl_bounds(records: &mut [Record], min: Option<u32>, max: Option<u32>) {
    for record in records {
        if let Some(min) = min {
            record.set_min_ttl(min);
        }
        if let Some(max) = max {
            record.set_max_ttl(max);
        }
    }
}

#[async_trait::async_trait]
impl Middleware<DnsContext, DnsRequest, DnsResponse, DnsError> for AddressMiddleware {
    async fn handle(
        &self,
        ctx: &mut DnsContext,
        req: &DnsRequest,
        next: Next<'_, DnsContext, DnsRequest, DnsResponse, DnsError>,
    ) -> Result<DnsResponse, DnsError> {
        let query_type = req.query().query_type();

        if let Some(rdatas) = handle_rule_addr(query_type, ctx) {
            let local_ttl = ctx.cfg().local_ttl();

            let query = req.query().original().clone();
            let name = query.name().to_owned();
            let valid_until = Instant::now() + Duration::from_secs(local_ttl);

            let lookup = DnsResponse::new_with_deadline(
                query,
                rdatas
                    .into_iter()
                    .map(|d| Record::from_rdata(name.clone(), local_ttl as u32, d)),
                valid_until,
            );

            ctx.source = LookupFrom::Static;
            return Ok(lookup);
        }

        let res = next.run(ctx, req).await;

        match res {
            Ok(lookup) => Ok({
                let mut records = Cow::Borrowed(lookup.records());

                if query_type.is_ip_addr()
                    && let Some(mut max_reply_ip_num) = ctx.cfg().max_reply_ip_num()
                    && max_reply_ip_num > 0
                {
                    let mut truncate = None;
                    for (i, r) in records.iter().enumerate() {
                        if matches!(r.data(), RData::A(_) | RData::AAAA(_)) {
                            max_reply_ip_num -= 1;
                            if max_reply_ip_num == 0 {
                                truncate = Some(i + 1);
                                break;
                            }
                        }
                    }

                    match truncate {
                        Some(truncate) if records.len() > truncate => {
                            records.to_mut().truncate(truncate);
                        }
                        _ => (),
                    }
                }

                let (rr_ttl_min, rr_ttl_max) = rr_ttl_bounds(ctx);
                let rr_ttl_reply_max = ctx.cfg().rr_ttl_reply_max().map(|i| i as u32);

                if rr_ttl_min.is_some() || rr_ttl_max.is_some() || rr_ttl_reply_max.is_some() {
                    let records = records.to_mut();
                    // min/max 与 C 的 _dns_server_get_conf_ttl 一致；reply-max 在此之上再压一次回复 TTL
                    clamp_ttl_bounds(records, rr_ttl_min, rr_ttl_max);
                    if let Some(rr_ttl_reply_max) = rr_ttl_reply_max {
                        for record in records {
                            record.set_max_ttl(rr_ttl_reply_max);
                        }
                    }
                }

                match records {
                    Cow::Owned(records) => {
                        // 重建 DnsResponse 时保留 from_cache 与 name_server_group 标记，
                        // 否则 new_with_deadline 会重置它们，导致上层缓存命中统计失真
                        let is_from_cache = lookup.is_from_cache();
                        let name_server_group = lookup.name_server_group().map(String::from);
                        let mut res = DnsResponse::new_with_deadline(
                            lookup.query().clone(),
                            records,
                            lookup.valid_until(),
                        );
                        if is_from_cache {
                            res.mark_from_cache();
                        }
                        if let Some(group) = name_server_group {
                            res = res.with_name_server_group(group);
                        }
                        res
                    }
                    Cow::Borrowed(_) => lookup,
                }
            }),
            Err(err) => Err(err),
        }
    }
}

fn handle_rule_addr(query_type: RecordType, ctx: &DnsContext) -> Option<Vec<RData>> {
    use RecordType::{A, AAAA, HTTPS};

    let cfg = ctx.cfg();
    let server_opts = ctx.server_opts();
    let rule = ctx.domain_rule.as_ref();

    let no_rule_soa = server_opts.no_rule_soa();

    // handle force SOA
    if !no_rule_soa {
        match query_type {
            // force AAAA query return SOA
            AAAA if server_opts.force_aaaa_soa() || cfg.force_aaaa_soa() => {
                return Some(vec![RData::default_soa()]);
            }
            // force HTTPS query return SOA
            HTTPS if server_opts.force_https_soa() || cfg.force_https_soa() => {
                return Some(vec![RData::default_soa()]);
            }
            _ => (),
        }

        // force specific qtype return SOA
        if cfg.force_qtype_soa().contains(&query_type) {
            return Some(vec![RData::default_soa()]);
        }
    }

    // skip address rule.
    if server_opts.no_rule_addr() || !query_type.is_ip_addr() {
        return None;
    }

    let mut node = rule;

    while let Some(rule) = node {
        use crate::dns_conf::AddressRuleValue::*;

        if let Some(address) = rule.address.as_ref() {
            match address {
                Addr { v4, v6 } => {
                    match query_type {
                        A => {
                            if let Some(v4) = v4 {
                                return Some(v4.iter().map(|ip| RData::A((*ip).into())).collect());
                            }
                        }
                        AAAA => {
                            if let Some(v6) = v6 {
                                return Some(
                                    v6.iter().map(|ip| RData::AAAA((*ip).into())).collect(),
                                );
                            }

                            if let Some(v4) = v4 {
                                return Some(
                                    v4.iter()
                                        .map(|ip| RData::AAAA(ip.to_ipv6_mapped().into()))
                                        .collect(),
                                );
                            }
                        }
                        _ => return None,
                    }
                    if !no_rule_soa {
                        return Some(vec![RData::default_soa()]);
                    }
                }
                SOA if !no_rule_soa => return Some(vec![RData::default_soa()]),
                SOAv4 if !no_rule_soa && query_type == A => {
                    return Some(vec![RData::default_soa()]);
                }
                SOAv6 if !no_rule_soa && query_type == AAAA => {
                    return Some(vec![RData::default_soa()]);
                }
                IGN => return None, // ignore rule
                IGNv4 if query_type == A => return None,
                IGNv6 if query_type == AAAA => return None,
                _ => (),
            };
        }

        node = rule.zone(); // find parent rule
    }

    None
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;

    use crate::{
        dns_conf::{AddressRuleValue, RuntimeConfig},
        dns_mw::*,
        libdns::proto::{
            op::{self, Edns, Query},
            rr::{Record, rdata, rdata::opt::ClientSubnet, rdata::opt::EdnsOption},
        },
    };

    #[tokio::test(flavor = "multi_thread")]
    async fn test_address_rule_soa_v6() {
        let cfg = RuntimeConfig::builder()
            .with("domain-rule /google.com/ -address #6")
            .build()
            .unwrap();

        assert_eq!(
            cfg.domain_rule_group("default")
                .find(&"google.com".parse().unwrap())
                .cloned()
                .unwrap()
                .address,
            Some(AddressRuleValue::SOAv6)
        );

        let mock = DnsMockMiddleware::mock(AddressMiddleware)
            .with_a_record("google.com", "8.8.8.8".parse().unwrap())
            .with_aaaa_record("google.com", "2001:4860:4860::8888".parse().unwrap())
            .build(cfg);

        assert!(matches!(
            mock.lookup_rdata("google.com", RecordType::AAAA)
                .await
                .unwrap()[0],
            RData::SOA(_)
        ));
        assert_eq!(
            mock.lookup_rdata("google.com", RecordType::A)
                .await
                .unwrap()[0],
            RData::A("8.8.8.8".parse().unwrap())
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_address_rule_soa2() {
        let cfg = RuntimeConfig::builder()
            .with(r#"domain-rule /google.com/ -address 1.2.3.4"#)
            .build()
            .unwrap();

        assert_eq!(
            cfg.domain_rule_group("default")
                .find(&"google.com".parse().unwrap())
                .cloned()
                .unwrap()
                .address,
            Some(AddressRuleValue::Addr {
                v4: Some(["1.2.3.4".parse().unwrap()].into()),
                v6: None
            })
        );

        let mock = DnsMockMiddleware::mock(AddressMiddleware)
            .with_a_record("google.com", "8.8.8.8".parse().unwrap())
            .with_aaaa_record("google.com", "2001:4860:4860::8888".parse().unwrap())
            .build(cfg);

        assert_eq!(
            mock.lookup_rdata("google.com", RecordType::A)
                .await
                .unwrap()[0],
            RData::A("1.2.3.4".parse().unwrap())
        );

        assert_eq!(
            mock.lookup_rdata("google.com", RecordType::AAAA)
                .await
                .unwrap()[0],
            RData::AAAA("::ffff:1.2.3.4".parse().unwrap())
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_client_rule_without_explicit_group_returns_group_address() {
        let cfg = RuntimeConfig::builder()
            .with("address /wiki.lan/192.168.1.5")
            .with("group-begin group-a")
            .with("client-rules 192.168.100.0/24")
            .with("address /wiki.lan/192.168.100.5")
            .with("group-end")
            .build()
            .unwrap();

        let mock = DnsMockMiddleware::mock(AddressMiddleware).build(cfg);

        let mut query = Query::query("wiki.lan".parse().unwrap(), RecordType::A);
        query.set_query_class(crate::libdns::proto::rr::DNSClass::IN);

        let mut message = op::Message::query();
        message.add_query(query.clone());
        let req_group_a = DnsRequest::new(
            message,
            "192.168.100.23:5300".parse().unwrap(),
            crate::libdns::Protocol::Udp,
        );

        let mut message = op::Message::query();
        message.add_query(query);
        let req_lan = DnsRequest::new(
            message,
            "192.168.1.23:5300".parse().unwrap(),
            crate::libdns::Protocol::Udp,
        );

        let res_group_a = mock
            .search(&req_group_a, &Default::default())
            .await
            .unwrap();
        let res_lan = mock.search(&req_lan, &Default::default()).await.unwrap();

        assert_eq!(
            res_group_a.records().first().unwrap().data(),
            &RData::A("192.168.100.5".parse().unwrap())
        );
        assert_eq!(
            res_lan.records().first().unwrap().data(),
            &RData::A("192.168.1.5".parse().unwrap())
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_client_rule_uses_edns_client_subnet_for_group_matching() {
        let cfg = RuntimeConfig::builder()
            .with("address /wiki.lan/192.168.1.5")
            .with("group-begin group-a")
            .with("client-rules 192.168.100.0/24")
            .with("address /wiki.lan/192.168.100.5")
            .with("group-end")
            .build()
            .unwrap();

        let mock = DnsMockMiddleware::mock(AddressMiddleware).build(cfg);

        let mut message = op::Message::query();
        message.add_query(Query::query("wiki.lan".parse().unwrap(), RecordType::A));

        let mut edns = Edns::new();
        edns.options_mut().insert(EdnsOption::Subnet(
            ClientSubnet::from_str("192.168.100.23/32").unwrap(),
        ));
        message.set_edns(edns);

        // ECS subnet should take precedence over the source address branch.
        let req = DnsRequest::new(
            message,
            "192.168.1.23:5300".parse().unwrap(),
            crate::libdns::Protocol::Udp,
        );

        let res = mock.search(&req, &Default::default()).await.unwrap();

        assert_eq!(
            res.records().first().unwrap().data(),
            &RData::A("192.168.100.5".parse().unwrap())
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_ttl_clip_ttl_min() -> Result<(), DnsError> {
        let cfg = RuntimeConfig::builder()
            .with("rr-ttl-min 50")
            .build()
            .unwrap();

        let mock = DnsMockMiddleware::mock(AddressMiddleware)
            .with_multi_records(
                "dns.google",
                RecordType::A,
                vec![
                    Record::from_rdata(
                        "dns.google".parse().unwrap(),
                        96,
                        RData::A("8.8.8.8".parse().unwrap()),
                    ),
                    Record::from_rdata(
                        "dns.google".parse().unwrap(),
                        48,
                        RData::A("8.8.4.4".parse().unwrap()),
                    ),
                ],
            )
            .build(cfg);

        let lookup = mock.lookup("dns.google", RecordType::A).await?;

        assert_eq!(lookup.min_ttl().unwrap(), 50);
        assert!(lookup.max_ttl().unwrap() > 50);

        Ok(())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_ttl_clip_ttl_max() -> Result<(), DnsError> {
        let cfg = RuntimeConfig::builder()
            .with("rr-ttl-max 50")
            .build()
            .unwrap();

        let mock = DnsMockMiddleware::mock(AddressMiddleware)
            .with_multi_records(
                "dns.google",
                RecordType::A,
                vec![
                    Record::from_rdata(
                        "dns.google".parse().unwrap(),
                        96,
                        RData::A("8.8.8.8".parse().unwrap()),
                    ),
                    Record::from_rdata(
                        "dns.google".parse().unwrap(),
                        48,
                        RData::A("8.8.4.4".parse().unwrap()),
                    ),
                ],
            )
            .build(cfg);

        let lookup = mock.lookup("dns.google", RecordType::A).await?;

        assert_eq!(lookup.max_ttl().unwrap(), 50);
        assert!(lookup.min_ttl().unwrap() < 50);

        Ok(())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_ttl_clip_ttl_min_max() -> Result<(), DnsError> {
        let cfg = RuntimeConfig::builder()
            .with("rr-ttl-max 66")
            .with("rr-ttl-min 55")
            .build()
            .unwrap();

        let mock = DnsMockMiddleware::mock(AddressMiddleware)
            .with_multi_records(
                "dns.google",
                RecordType::A,
                vec![
                    Record::from_rdata(
                        "dns.google".parse().unwrap(),
                        96,
                        RData::A("8.8.8.8".parse().unwrap()),
                    ),
                    Record::from_rdata(
                        "dns.google".parse().unwrap(),
                        48,
                        RData::A("8.8.4.4".parse().unwrap()),
                    ),
                ],
            )
            .build(cfg);

        let lookup = mock.lookup("dns.google", RecordType::A).await?;

        assert_eq!(lookup.max_ttl().unwrap(), 66);
        assert_eq!(lookup.min_ttl().unwrap(), 55);

        Ok(())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_ttl_clip_ttl_max_reply() -> Result<(), DnsError> {
        let cfg = RuntimeConfig::builder()
            .with("rr-ttl-max 66")
            .with("rr-ttl-min 55")
            .with("rr-ttl-reply-max 30")
            .build()
            .unwrap();

        let mock = DnsMockMiddleware::mock(AddressMiddleware)
            .with_multi_records(
                "dns.google",
                RecordType::A,
                vec![
                    Record::from_rdata(
                        "dns.google".parse().unwrap(),
                        96,
                        RData::A("8.8.8.8".parse().unwrap()),
                    ),
                    Record::from_rdata(
                        "dns.google".parse().unwrap(),
                        48,
                        RData::A("8.8.4.4".parse().unwrap()),
                    ),
                ],
            )
            .build(cfg);

        let lookup = mock.lookup("dns.google", RecordType::A).await?;

        assert!(lookup.record_iter().all(|r| r.ttl() == 30));

        Ok(())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_ttl_clip_ttl_max_reply_ip_num() -> Result<(), DnsError> {
        let cfg = RuntimeConfig::builder()
            .with("rr-ttl-max 66")
            .with("rr-ttl-min 55")
            .with("rr-ttl-reply-max 30")
            .with("max-reply-ip-num 2")
            .build()
            .unwrap();

        let mock = DnsMockMiddleware::mock(AddressMiddleware)
            .with_multi_records(
                "dns.google",
                RecordType::A,
                vec![
                    Record::from_rdata(
                        "dns.google".parse().unwrap(),
                        96,
                        RData::A("8.8.8.8".parse().unwrap()),
                    ),
                    Record::from_rdata(
                        "dns.google".parse().unwrap(),
                        48,
                        RData::A("8.8.4.4".parse().unwrap()),
                    ),
                    Record::from_rdata(
                        "dns.google".parse().unwrap(),
                        48,
                        RData::A("8.8.4.3".parse().unwrap()),
                    ),
                ],
            )
            .build(cfg);

        let lookup = mock.lookup("dns.google", RecordType::A).await?;

        assert_eq!(lookup.records().len(), 2);

        Ok(())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_ttl_clip_ttl_max_reply_ip_num_1() -> Result<(), DnsError> {
        let cfg = RuntimeConfig::builder()
            .with("rr-ttl-max 66")
            .with("rr-ttl-min 55")
            .with("rr-ttl-reply-max 30")
            .with("max-reply-ip-num 1")
            .build()
            .unwrap();

        let mock = DnsMockMiddleware::mock(AddressMiddleware)
            .with_multi_records(
                "dns.google",
                RecordType::A,
                vec![Record::from_rdata(
                    "dns.google".parse().unwrap(),
                    96,
                    RData::A("8.8.8.8".parse().unwrap()),
                )],
            )
            .build(cfg);

        let lookup = mock.lookup("dns.google", RecordType::A).await?;

        assert_eq!(lookup.records().len(), 1);

        Ok(())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_ttl_clip_ttl_max_reply_ip_num_2() -> Result<(), DnsError> {
        let cfg = RuntimeConfig::builder()
            .with("rr-ttl-max 66")
            .with("rr-ttl-min 55")
            .with("rr-ttl-reply-max 30")
            .with("max-reply-ip-num 2")
            .build()
            .unwrap();

        let mock = DnsMockMiddleware::mock(AddressMiddleware)
            .with_multi_records(
                "dns.google",
                RecordType::A,
                vec![
                    Record::from_rdata(
                        "dns.google".parse().unwrap(),
                        96,
                        RData::A("8.8.8.8".parse().unwrap()),
                    ),
                    Record::from_rdata(
                        "dns.google".parse().unwrap(),
                        48,
                        RData::A("8.8.4.4".parse().unwrap()),
                    ),
                    Record::from_rdata(
                        "dns.google".parse().unwrap(),
                        48,
                        RData::A("8.8.4.3".parse().unwrap()),
                    ),
                ],
            )
            .build(cfg);

        let lookup = mock.lookup("dns.google", RecordType::A).await?;

        assert_eq!(lookup.records().len(), 2);

        Ok(())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_ttl_clip_ttl_cname_max_reply_ip_num_2() -> Result<(), DnsError> {
        let cfg = RuntimeConfig::builder()
            .with("rr-ttl-max 66")
            .with("rr-ttl-min 55")
            .with("rr-ttl-reply-max 30")
            .with("max-reply-ip-num 2")
            .build()
            .unwrap();

        let mock = DnsMockMiddleware::mock(AddressMiddleware)
            .with_multi_records(
                "dns.google",
                RecordType::A,
                vec![
                    Record::from_rdata(
                        "dns.google".parse().unwrap(),
                        96,
                        RData::CNAME(rdata::CNAME("dns.google".parse::<Name>().unwrap())),
                    ),
                    Record::from_rdata(
                        "dns.google".parse().unwrap(),
                        96,
                        RData::A("8.8.8.8".parse().unwrap()),
                    ),
                    Record::from_rdata(
                        "dns.google".parse().unwrap(),
                        48,
                        RData::A("8.8.4.4".parse().unwrap()),
                    ),
                ],
            )
            .build(cfg);

        let lookup = mock.lookup("dns.google", RecordType::A).await?;

        let ip_count: u8 = lookup
            .record_iter()
            .map(|r| matches!(r.data(), RData::A(_) | RData::AAAA(_)) as u8)
            .sum();

        assert_eq!(ip_count, 2);

        Ok(())
    }

    /// 回归测试：缓存命中的响应经过 TTL 调整（Cow::Owned 重建）后，
    /// from_cache 标记必须保留，否则上层缓存命中统计会失真
    #[tokio::test(flavor = "multi_thread")]
    async fn test_from_cache_preserved_after_ttl_clip() -> Result<(), DnsError> {
        let cfg = RuntimeConfig::builder()
            .with("rr-ttl-min 50")
            .build()
            .unwrap();

        let mock = DnsMockMiddleware::mock(AddressMiddleware)
            .with_a_record_from_cache("dns.google", "8.8.8.8".parse().unwrap(), 96)
            .build(cfg);

        let lookup = mock.lookup("dns.google", RecordType::A).await?;

        assert!(lookup.is_from_cache(), "from_cache 标记在 TTL 调整后丢失");

        Ok(())
    }

    /// 回归测试：缓存命中的响应经过 max-reply-ip-num 截断（Cow::Owned 重建）后，
    /// from_cache 标记必须保留
    #[tokio::test(flavor = "multi_thread")]
    async fn test_from_cache_preserved_after_truncate() -> Result<(), DnsError> {
        let cfg = RuntimeConfig::builder()
            .with("max-reply-ip-num 2")
            .build()
            .unwrap();

        let mock = DnsMockMiddleware::mock(AddressMiddleware)
            .with_a_record_from_cache("dns.google", "8.8.8.8".parse().unwrap(), 96)
            .build(cfg);

        let lookup = mock.lookup("dns.google", RecordType::A).await?;

        assert!(lookup.is_from_cache(), "from_cache 标记在截断后丢失");

        Ok(())
    }

    /// rr-ttl / rr-ttl-min / rr-ttl-max 会折叠成同一对边界；rr-ttl-reply-max 不参与。
    #[test]
    fn test_rr_ttl_bounds_folds_fixed_ttl() {
        let cfg = RuntimeConfig::builder()
            .with("rr-ttl-min 300")
            .with("rr-ttl-max 600")
            .build()
            .unwrap();
        let ctx = DnsContext::new(
            &"example.com".parse().unwrap(),
            std::sync::Arc::new(cfg),
            Default::default(),
        );
        assert_eq!(rr_ttl_bounds(&ctx), (Some(300), Some(600)));

        let cfg = RuntimeConfig::builder().with("rr-ttl 120").build().unwrap();
        let ctx = DnsContext::new(
            &"example.com".parse().unwrap(),
            std::sync::Arc::new(cfg),
            Default::default(),
        );
        assert_eq!(rr_ttl_bounds(&ctx), (Some(120), Some(120)));

        let cfg = RuntimeConfig::builder()
            .with("rr-ttl-reply-max 60")
            .build()
            .unwrap();
        let ctx = DnsContext::new(
            &"example.com".parse().unwrap(),
            std::sync::Arc::new(cfg),
            Default::default(),
        );
        assert_eq!(rr_ttl_bounds(&ctx), (None, None), "reply-max 只压回复");
    }

    /// 夹取顺序：先 min 后 max，结果等于 min(max(ttl, min), max)。
    #[test]
    fn test_clamp_ttl_bounds() {
        let name: crate::libdns::proto::rr::Name = "example.com".parse().unwrap();
        let mut records = vec![
            Record::from_rdata(
                name.clone(),
                5,
                RData::A("192.0.2.1".parse::<std::net::Ipv4Addr>().unwrap().into()),
            ),
            Record::from_rdata(
                name,
                900,
                RData::A("192.0.2.2".parse::<std::net::Ipv4Addr>().unwrap().into()),
            ),
        ];
        clamp_ttl_bounds(&mut records, Some(300), Some(600));
        assert_eq!(records[0].ttl(), 300);
        assert_eq!(records[1].ttl(), 600);
    }
}
