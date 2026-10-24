use std::time::{Duration, Instant};

use crate::libdns::proto::rr::rdata::CNAME;

use crate::config::CNameRule;
use crate::dns::*;
use crate::middleware::*;

pub struct DnsCNameMiddleware;

/// `force-no-CNAME yes`：应答只保留解析结果，裁掉 CNAME 记录。
pub struct DnsForceNoCnameMiddleware;

#[async_trait::async_trait]
impl Middleware<DnsContext, DnsRequest, DnsResponse, DnsError> for DnsForceNoCnameMiddleware {
    async fn handle(
        &self,
        ctx: &mut DnsContext,
        req: &DnsRequest,
        next: Next<'_, DnsContext, DnsRequest, DnsResponse, DnsError>,
    ) -> Result<DnsResponse, DnsError> {
        let mut lookup = next.run(ctx, req).await?;

        // CNAME 查询本身的答案就是 CNAME 记录，不能裁
        if req.query().query_type() != RecordType::CNAME {
            strip_cname_records(&mut lookup);
        }

        Ok(lookup)
    }
}

fn strip_cname_records(lookup: &mut DnsResponse) {
    lookup
        .answers_mut()
        .retain(|record| record.record_type() != RecordType::CNAME);
}

#[async_trait::async_trait]
impl Middleware<DnsContext, DnsRequest, DnsResponse, DnsError> for DnsCNameMiddleware {
    async fn handle(
        &self,
        ctx: &mut DnsContext,
        req: &DnsRequest,
        next: Next<'_, DnsContext, DnsRequest, DnsResponse, DnsError>,
    ) -> Result<DnsResponse, DnsError> {
        let cname = match &ctx.domain_rule {
            Some(rule) => rule.get(|r| match &r.cname {
                Some(cname) => match cname {
                    CNameRule::Ignore => None,
                    CNameRule::Value(n) => Some(n.clone()),
                },
                None => None,
            }),
            None => None,
        };

        match cname {
            Some(mut cname) => {
                if !cname.is_fqdn() {
                    cname.set_fqdn(true);
                }

                let local_ttl = ctx.cfg().local_ttl();

                let cname_record = Record::from_rdata(
                    req.query().original().name().clone(),
                    local_ttl as u32,
                    RData::CNAME(CNAME(cname.clone())),
                );

                if req.query().query_type() == RecordType::CNAME {
                    let query = req.query().original().clone();
                    let valid_until = Instant::now() + Duration::from_secs(local_ttl);

                    Ok(DnsResponse::new_with_deadline(
                        query,
                        vec![cname_record],
                        valid_until,
                    ))
                } else {
                    let mut ctx =
                        DnsContext::new(&cname, ctx.cfg().clone(), ctx.server_opts().clone());

                    let new_req = req.with_cname(cname.clone());
                    match next.run(&mut ctx, &new_req).await {
                        Ok(mut lookup) => {
                            *lookup.queries_mut() = vec![req.query().original().clone()];
                            lookup.answers_mut().insert(0, cname_record);
                            Ok(lookup)
                        }
                        Err(err) => Err(err),
                    }
                }
            }
            None => next.run(ctx, req).await,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::libdns::proto::{
        op::Query,
        rr::{RData, Record, rdata::CNAME},
    };

    #[test]
    fn test_strip_cname_records() {
        let query = Query::query("www.example.com".parse().unwrap(), RecordType::A);
        let records = vec![
            Record::from_rdata(
                "www.example.com".parse().unwrap(),
                60,
                RData::CNAME(CNAME("target.example.com".parse().unwrap())),
            ),
            Record::from_rdata(
                "target.example.com".parse().unwrap(),
                60,
                RData::A(std::net::Ipv4Addr::new(192, 0, 2, 1).into()),
            ),
        ];

        let mut response = DnsResponse::new_with_max_ttl(query, records);
        strip_cname_records(&mut response);

        assert_eq!(response.answers().len(), 1);
        assert_eq!(response.answers()[0].record_type(), RecordType::A);
    }
}
