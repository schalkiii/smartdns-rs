use crate::config::HttpsRecordRule;
use crate::dns::*;
use crate::libdns::proto::rr::rdata::TXT;
use crate::middleware::Next;

pub struct RuleZoneProvider;

impl RuleZoneProvider {
    pub fn new() -> Self {
        Self
    }

    pub async fn lookup(
        &self,
        ctx: &mut DnsContext,
        req: &DnsRequest,
        next: Next<'_, DnsContext, DnsRequest, DnsResponse, DnsError>,
    ) -> Result<Option<DnsResponse>, DnsError> {
        match req.query().query_type() {
            RecordType::SRV => {
                if let Some(values) = ctx.domain_rule.get_ref(|r| r.srv.as_ref()) {
                    let query = req.query().original().to_owned();
                    let records = values
                        .iter()
                        .map(|srv| {
                            let mut record = Record::from_rdata(
                                query.name().to_owned(),
                                crate::dns_client::MAX_TTL,
                                RData::SRV(srv.clone()),
                            );
                            record.set_dns_class(query.query_class());
                            record
                        })
                        .collect::<Vec<_>>();

                    return Ok(Some(DnsResponse::new_with_max_ttl(query, records)));
                }
            }
            RecordType::HTTPS => {
                if let Some(https_rule) = ctx.domain_rule.get_ref(|r| r.https.as_ref()) {
                    match https_rule {
                        HttpsRecordRule::Ignore => (),
                        HttpsRecordRule::SOA => {
                            return Ok(Some(DnsResponse::from_rdata(
                                req.query().original().to_owned(),
                                RData::default_soa(),
                            )));
                        }
                        HttpsRecordRule::Filter {
                            no_ipv4_hint,
                            no_ipv6_hint,
                        } => {
                            use crate::libdns::proto::rr::rdata::{SVCB, svcb::SvcParamKey};
                            let no_ipv4_hint = *no_ipv4_hint;
                            let no_ipv6_hint = *no_ipv6_hint;

                            let mut lookup = next.run(ctx, req).await?;
                            for record in lookup.answers_mut() {
                                if let Some(https) = record.data_mut().as_https_mut() {
                                    let svc_params = https
                                        .svc_params()
                                        .iter()
                                        .filter(|(k, _)| match k {
                                            SvcParamKey::Ipv4Hint => !no_ipv4_hint,
                                            SvcParamKey::Ipv6Hint => !no_ipv6_hint,
                                            _ => true,
                                        })
                                        .cloned()
                                        .collect();

                                    https.0 = SVCB::new(
                                        https.svc_priority(),
                                        https.target_name().clone(),
                                        svc_params,
                                    );
                                }
                            }
                            return Ok(Some(lookup));
                        }
                        HttpsRecordRule::RecordData(https) => {
                            return Ok(Some(DnsResponse::from_rdata(
                                req.query().original().to_owned(),
                                RData::HTTPS(https.clone()),
                            )));
                        }
                    }
                }
            }
            RecordType::TXT => {
                if let Some(values) = ctx.domain_rule.get_ref(|r| r.txt.as_ref()) {
                    let records = values
                        .iter()
                        .map(|value| {
                            Record::from_rdata(
                                req.query().original().name().to_owned(),
                                crate::dns_client::MAX_TTL,
                                RData::TXT(TXT::new(vec![value.clone()])),
                            )
                        })
                        .collect::<Vec<_>>();

                    return Ok(Some(DnsResponse::new_with_max_ttl(
                        req.query().original().to_owned(),
                        records,
                    )));
                }
            }
            _ => (),
        }

        Ok(None)
    }
}
