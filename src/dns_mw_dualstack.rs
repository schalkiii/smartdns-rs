use std::net::IpAddr;
use std::time::Duration;

use futures::FutureExt;
use futures::future::{Either, select};

use crate::config::SpeedCheckMode;
use crate::dns::*;
use crate::log::debug;
use crate::middleware::*;
use crate::third_ext::FutureTimeoutExt;

pub struct DnsDualStackIpSelectionMiddleware;

/// 两族测速的**整体**预算（不是每族各 200ms）：超预算即视为测不出结果。
const PING_BUDGET: Duration = Duration::from_millis(200);

/// 等另一族应答的上限。客户端 Happy Eyeballs 通常在 ~250ms 内就切到可用族，
/// 等到 2s 没有意义；取 500ms：正常 AAAA 都在这之内，超时即按「对方更快」返回空，
/// 既能兜住异常链路的尾延迟，也不会误杀健康但稍慢的 AAAA。
const OTHER_FAMILY_WAIT_CAP: Duration = Duration::from_millis(500);

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
            .or_else(|| ctx.cfg().speed_check_mode().cloned())
            .unwrap_or_default();

        // 没有任何测速模式时无法比较两族延迟，继续并发另一族只是白等
        if speed_check_mode.is_empty() {
            return next.run(ctx, req).await;
        }

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
                Ok(that) => match tokio::time::timeout(OTHER_FAMILY_WAIT_CAP, this).await {
                    Ok(Ok(this)) => {
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
                    Ok(Err(err)) => Err(err),
                    // 对方先到时本族仍未返回：按「对方更快」处理，AAAA 查询返回空，
                    // 让客户端立刻走 A，而不是被异常链路拖住。
                    Err(_) => this_no_records(),
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

    let deadline = tokio::time::Instant::now() + PING_BUDGET;
    let this_ping = multi_mode_ping_fastest(this_ip_addrs, modes.to_vec(), deadline).boxed();
    let that_ping = multi_mode_ping_fastest(that_ip_addrs, modes.to_vec(), deadline).boxed();

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
    deadline: tokio::time::Instant,
) -> Option<(IpAddr, Duration)> {
    use crate::infra::ping::{PingOptions, ping_fastest};
    let ping_ops = PingOptions::default().with_timeout_secs(2);

    let mut fastest_ip = None;

    for mode in &modes {
        let dests = mode.to_ping_addrs(&ip_addrs);

        // 整体预算：所有模式共享同一个 deadline，而不是每种模式各等 200ms
        let ping_task = ping_fastest(dests, ping_ops).boxed();
        match tokio::time::timeout_at(deadline, ping_task).await {
            Ok(Ok(ping_out)) => {
                let ip = ping_out.dest().ip_addr();
                let duration = ping_out.elapsed();
                fastest_ip = Some((ip, duration));
                break;
            }
            // 该模式没测出结果，换下一个模式（仍受同一 deadline 约束）
            Ok(Err(_)) => continue,
            // 整体预算耗尽
            Err(_) => break,
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
