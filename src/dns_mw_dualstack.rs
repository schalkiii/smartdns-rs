use std::net::IpAddr;
use std::time::Duration;

use futures::FutureExt;
use futures::future::{Either, select};
use tokio::time::sleep;

use crate::config::SpeedCheckMode;
use crate::dns::*;
use crate::log::debug;
use crate::middleware::*;
use crate::third_ext::FutureTimeoutExt;

pub struct DnsDualStackIpSelectionMiddleware;

#[async_trait::async_trait]
impl Middleware<DnsContext, DnsRequest, DnsResponse, DnsError>
    for DnsDualStackIpSelectionMiddleware
{
    async fn handle(
        &self,
        ctx: &mut DnsContext,
        req: &DnsRequest,
        next: Next<'_, DnsContext, DnsRequest, DnsResponse, DnsError>,
    ) -> Result<DnsResponse, DnsError> {
        use RecordType::{A, AAAA};

        // highest priority
        if ctx.server_opts.no_dualstack_selection() {
            return next.run(ctx, req).await;
        }

        let query_type = req.query().query_type();

        // must be ip query.
        if !query_type.is_ip_addr() {
            return next.run(ctx, req).await;
        }

        let mut prefer_that = false; // As long as it succeeds, there is no need to check the selection threshold.

        if matches!(query_type, A) {
            if ctx.cfg().dualstack_ip_allow_force_aaaa() {
                prefer_that = true;
            } else {
                return next.run(ctx, req).await;
            }
        }

        // read config
        let dualstack_ip_selection = ctx
            .domain_rule
            .as_ref()
            .map(|rule| rule.dualstack_ip_selection)
            .unwrap_or_default()
            .unwrap_or(ctx.cfg().dualstack_ip_selection());

        if !dualstack_ip_selection {
            return next.run(ctx, req).await;
        }

        let selection_threshold =
            Duration::from_millis(ctx.cfg().dualstack_ip_selection_threshold());

        let speed_check_mode = ctx
            .domain_rule
            .get_ref(|r| r.speed_check_mode.as_ref())
            .cloned()
            .unwrap_or_default();

        let ttl = ctx.cfg().rr_ttl().unwrap_or_default() as u32;

        let that_type = match query_type {
            A => AAAA,
            AAAA => A,
            typ => typ,
        };

        let mut that_ctx = ctx.clone();
        let that_req = {
            let mut req = req.clone();
            req.set_query_type(that_type);
            req
        };

        let that = next.clone().run(&mut that_ctx, &that_req);
        let this = next.run(ctx, req);

        let dual_task = futures::future::select(this, that).await;

        let this_no_records = || {
            debug!(
                "dual stack IP selection: {} , choose {}",
                req.query().name(),
                that_type
            );
            Err(DnsError::no_records_found(
                req.query().original().to_owned(),
                ttl,
            ))
        };

        match dual_task {
            Either::Left((res, that)) => match res {
                Ok(this) => {
                    let that = that.timeout(selection_threshold).await;

                    if let Ok(Ok(that)) = that {
                        let that_faster = matches!(
                            which_faster(&this, &that, &speed_check_mode, selection_threshold)
                                .await,
                            Either::Right(_)
                        );

                        if that_faster && (prefer_that || matches!(query_type, AAAA)) {
                            return this_no_records();
                        }
                    }

                    Ok(this)
                }
                Err(err) => Err(err),
            },
            Either::Right((res, this)) => match res {
                Ok(that) => match this.await {
                    Ok(this) => {
                        let that_faster = matches!(
                            which_faster(&this, &that, &speed_check_mode, selection_threshold)
                                .await,
                            Either::Right(_)
                        );

                        if that_faster && (prefer_that || matches!(query_type, AAAA)) {
                            return this_no_records();
                        }
                        Ok(this)
                    }
                    Err(err) => Err(err),
                },
                Err(_) => this.await,
            },
        }
    }
}

async fn which_faster(
    this: &DnsResponse,
    that: &DnsResponse,
    modes: &[SpeedCheckMode],
    selection_threshold: Duration,
) -> Either<(), ()> {
    let this_ip_addrs = this.ip_addrs();
    let that_ip_addrs = that.ip_addrs();

    let this_ping = multi_mode_ping_fastest(this_ip_addrs, modes.to_vec()).boxed();
    let that_ping = multi_mode_ping_fastest(that_ip_addrs, modes.to_vec()).boxed();

    let which_faster = select(this_ping, that_ping).await;

    let that_faster = match which_faster {
        Either::Right((that_res, this_ping)) => {
            let this_dura = this_ping.await.map(|(_, d)| d);
            decide_that_faster(this_dura, that_res.map(|(_, d)| d), selection_threshold)
        }
        Either::Left(_) => false,
    };

    if that_faster {
        Either::Right(())
    } else {
        Either::Left(())
    }
}

async fn multi_mode_ping_fastest(
    ip_addrs: Vec<IpAddr>,
    modes: Vec<SpeedCheckMode>,
) -> Option<(IpAddr, Duration)> {
    use crate::infra::ping::{PingOptions, ping_fastest};
    let duration = Duration::from_millis(200);
    let ping_ops = PingOptions::default().with_timeout_secs(2);

    let mut fastest_ip = None;

    for mode in &modes {
        let dests = mode.to_ping_addrs(&ip_addrs);

        let ping_task = ping_fastest(dests, ping_ops).boxed();
        let timeout_task = sleep(duration).boxed();
        match futures_util::future::select(ping_task, timeout_task).await {
            futures::future::Either::Left((ping_res, _)) => {
                match ping_res {
                    Ok(ping_out) => {
                        // ping success
                        let ip = ping_out.dest().ip_addr();
                        let duration = ping_out.elapsed();
                        fastest_ip = Some((ip, duration));
                        break;
                    }
                    Err(_) => continue,
                }
            }
            futures::future::Either::Right((_, _)) => {
                // timeout
                continue;
            }
        }
    }

    fastest_ip
}

/// 「另一地址族明显更快」的判据：不仅更快，且差值严格超过 `dualstack-ip-selection-threshold`，
/// 避免延迟抖动导致 A/AAAA 来回切换。
fn is_that_faster(this_dura: Duration, that_dura: Duration, threshold: Duration) -> bool {
    this_dura > that_dura && (this_dura - that_dura) > threshold
}

/// 结合两族的测速结果给出最终判定：对方没测出结果就不切换；本族没结果、对方有则切换。
fn decide_that_faster(
    this_dura: Option<Duration>,
    that_dura: Option<Duration>,
    threshold: Duration,
) -> bool {
    match (this_dura, that_dura) {
        (_, None) => false,
        (None, Some(_)) => true,
        (Some(this_dura), Some(that_dura)) => is_that_faster(this_dura, that_dura, threshold),
    }
}

#[cfg(test)]
mod tests {
    use super::{decide_that_faster, is_that_faster};
    use std::time::Duration;

    #[test]
    fn test_is_that_faster() {
        let ms = Duration::from_millis;
        assert!(
            is_that_faster(ms(100), ms(50), ms(20)),
            "差值超过阈值应判更快"
        );
        assert!(
            !is_that_faster(ms(100), ms(50), ms(50)),
            "差值等于阈值不切换（严格大于）"
        );
        assert!(!is_that_faster(ms(50), ms(100), ms(20)), "本族更快不应切换");
        assert!(
            !is_that_faster(ms(60), ms(50), ms(20)),
            "差值小于阈值不切换"
        );
    }

    /// 两族测速结果合并判定：含「对方无结果」「本族无结果」两类边界。
    #[test]
    fn test_decide_that_faster() {
        let ms = Duration::from_millis;
        assert!(
            decide_that_faster(Some(ms(100)), Some(ms(50)), ms(20)),
            "对方更快且超过阈值"
        );
        assert!(
            decide_that_faster(None, Some(ms(50)), ms(20)),
            "本族无结果、对方有"
        );
        assert!(
            !decide_that_faster(Some(ms(50)), None, ms(20)),
            "对方无结果不切换"
        );
        assert!(
            !decide_that_faster(None, None, ms(20)),
            "双方都无结果不切换"
        );
    }
}
