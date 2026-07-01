use std::collections::HashMap;
use std::ops::Deref;
use std::sync::Arc;
use std::{borrow::Borrow, net::IpAddr, time::Duration};

use crate::dns_client::{LookupOptions, NameServer};

use crate::infra::ipset::{IpMap, IpSet};
use crate::infra::ping::{PingError, PingOutput};
use crate::{
    config::{ResponseMode, SpeedCheckMode, SpeedCheckModeList},
    dns::*,
    dns_client::{DnsClient, GenericResolver, NameServerGroup},
    dns_error::LookupError,
    log::{debug, error, trace},
    middleware::*,
};

use crate::libdns::proto::rr::domain::usage::LOCAL;
use crate::libdns::proto::{AuthorityData, op::ResponseCode, rr::rdata::opt::EdnsCode};
use futures::stream::FuturesUnordered;
use futures::{FutureExt, StreamExt, future::BoxFuture};
use rr::rdata::opt::EdnsOption;
use tokio::time::sleep;

pub struct NameServerMiddleware {
    client: DnsClient,
}

impl NameServerMiddleware {
    pub fn new(client: DnsClient) -> Self {
        Self { client }
    }
}

#[async_trait::async_trait]
impl Middleware<DnsContext, DnsRequest, DnsResponse, DnsError> for NameServerMiddleware {
    #[inline]
    async fn handle(
        &self,
        ctx: &mut DnsContext,
        req: &DnsRequest,
        _next: crate::middleware::Next<'_, DnsContext, DnsRequest, DnsResponse, DnsError>,
    ) -> Result<DnsResponse, DnsError> {
        let name: &Name = req.query().name().borrow();
        let rtype = req.query().query_type();

        let client = &self.client;

        if rtype.is_ip_addr()
            && let Some(lookup) = client.lookup_nameserver(name.clone(), rtype).await
        {
            debug!(
                "[ns] lookup {} {} -> {:?}",
                name,
                rtype,
                lookup
                    .answers()
                    .iter()
                    .filter_map(|record| record.data().ip_addr())
                    .collect::<Vec<_>>()
            );
            ctx.no_cache = true;
            return Ok(lookup);
        }

        let lookup_options = LookupOptions {
            is_dnssec: req.is_dnssec(),
            record_type: rtype,
            client_subnet: req
                .extensions()
                .as_ref()
                .and_then(|edns| {
                    edns.option(EdnsCode::Subnet).and_then(|opt| match opt {
                        EdnsOption::Subnet(subnet) => Some(*subnet),
                        _ => None,
                    })
                })
                .or_else(|| ctx.domain_rule.get_ref(|r| r.subnet.as_ref()).cloned()),
        };

        // skip nameserver rule
        if ctx.server_opts.no_rule_nameserver() {
            return client.lookup(name.clone(), lookup_options).await;
        }

        let group_name = ctx.server_group_name().to_string();

        let name_server = {
            let name_server = if ctx.cfg().mdns_lookup() && LOCAL.zone_of(name) {
                client.get_server_group("mdns").await
            } else {
                None
            };

            let name_server = match name_server {
                Some(ns) => Some(ns),
                None => client.get_server_group(group_name.as_ref()).await,
            };

            match name_server {
                Some(ns) => ns,
                None => {
                    error!("no available nameserver found for {}", name);
                    return Err(ProtoErrorKind::NoConnections.into());
                }
            }
        };

        trace!(
            "query name: {} type: {}{} via [Group: {}]",
            name,
            rtype,
            match lookup_options.client_subnet.as_ref() {
                Some(subnet) => format!("\tsubnet: {}/{}", subnet.addr(), subnet.scope_prefix()),
                None => String::with_capacity(0),
            },
            group_name
        );

        ctx.source = LookupFrom::Server(group_name.to_string());

        if rtype.is_ip_addr() {
            let cfg = ctx.cfg();

            let mut opts = match ctx.domain_rule.as_ref() {
                Some(rule) => LookupIpOptions {
                    response_strategy: rule
                        .get(|n| n.response_mode)
                        .unwrap_or_else(|| cfg.response_mode()),
                    speed_check_mode: match rule.speed_check_mode.as_ref() {
                        Some(mode) => Some(mode.clone()),
                        None => cfg.speed_check_mode().cloned(),
                    },
                    no_speed_check: ctx.server_opts.no_speed_check(),
                    ignore_ip: cfg.ignore_ip().clone(),
                    blacklist_ip: cfg.blacklist_ip().clone(),
                    whitelist_ip: cfg.whitelist_ip().clone(),
                    ip_alias: cfg.ip_alias().clone(),
                    lookup_options,
                },
                None => LookupIpOptions {
                    response_strategy: cfg.response_mode(),
                    speed_check_mode: cfg.speed_check_mode().cloned(),
                    no_speed_check: ctx.server_opts.no_speed_check(),
                    ignore_ip: cfg.ignore_ip().clone(),
                    blacklist_ip: cfg.blacklist_ip().clone(),
                    whitelist_ip: cfg.whitelist_ip().clone(),
                    ip_alias: cfg.ip_alias().clone(),
                    lookup_options,
                },
            };

            if ctx.server_opts.is_background {
                opts.response_strategy = ResponseMode::FastestIp;
            }

            lookup_ip(name_server.deref(), name.clone(), &opts).await
        } else {
            name_server.lookup(name.clone(), lookup_options).await
        }
        .map(|res| res.with_name_server_group(group_name.to_string()))
    }
}

struct LookupIpOptions {
    response_strategy: ResponseMode,
    speed_check_mode: Option<SpeedCheckModeList>,
    no_speed_check: bool,
    ignore_ip: Arc<IpSet>,
    whitelist_ip: Arc<IpSet>,
    blacklist_ip: Arc<IpSet>,
    ip_alias: Arc<IpMap<Arc<[IpAddr]>>>,
    lookup_options: LookupOptions,
}

impl Deref for LookupIpOptions {
    type Target = LookupOptions;

    fn deref(&self) -> &Self::Target {
        &self.lookup_options
    }
}

impl From<LookupIpOptions> for LookupOptions {
    fn from(value: LookupIpOptions) -> Self {
        value.lookup_options
    }
}

impl From<&LookupIpOptions> for LookupOptions {
    fn from(value: &LookupIpOptions) -> Self {
        value.lookup_options.clone()
    }
}

/// IP 查询策略执行结果
enum IpStrategyResult {
    /// 直接返回完整 DNS 响应（FastestResponse 策略或 FirstPing 单 IP 场景）
    DirectResponse(Box<DnsResponse>),
    /// 选出了最优 IP，附带成功和失败的任务结果
    SelectedIp(IpAddr, Vec<DnsResponse>, Vec<DnsError>),
    /// 无明确最优 IP，回退到按响应顺序返回
    Fallback(Vec<DnsResponse>, Vec<DnsError>),
    Error(LookupError),
}

/// 将 IpStrategyResult 转换为标准 Result<DnsResponse, LookupError>
fn handle_strategy_result(result: IpStrategyResult) -> Result<DnsResponse, LookupError> {
    match result {
        IpStrategyResult::DirectResponse(response) => Ok(*response),
        IpStrategyResult::SelectedIp(selected_ip, ok_tasks, _err_tasks) => {
            for mut res in ok_tasks {
                let record = res
                    .take_answers()
                    .into_iter()
                    .find(|r| matches!(r.data().ip_addr(), Some(ip) if ip == selected_ip));
                if let Some(record) = record {
                    res.add_answer(record);
                    return Ok(res);
                }
            }
            unreachable!("selected_ip 必定存在于 ok_tasks 中")
        }
        IpStrategyResult::Fallback(ok_tasks, err_tasks) => match ok_tasks.into_iter().next() {
            Some(lookup) => Ok(lookup),
            None => match err_tasks.into_iter().next() {
                Some(err) => Err(err),
                None => unreachable!(),
            },
        },
        IpStrategyResult::Error(err) => Err(err),
    }
}

async fn lookup_ip(
    server: &NameServerGroup,
    name: Name,
    options: &LookupIpOptions,
) -> Result<DnsResponse, LookupError> {
    use ResponseMode::*;

    assert!(options.record_type.is_ip_addr());

    if server.is_empty() {
        return Err(ProtoErrorKind::NoConnections.into());
    }

    // 速度检查忽略逻辑
    let response_strategy = if options.no_speed_check || options.speed_check_mode.is_none() {
        FastestResponse
    } else {
        options.response_strategy
    };

    let speed_check_mode = options
        .speed_check_mode
        .as_ref()
        .map(|m| m.as_slice())
        .unwrap_or_default();

    let (response_strategy, speed_check_mode) = if speed_check_mode.iter().any(|m| m.is_none()) {
        (FastestResponse, &[][..])
    } else {
        (response_strategy, speed_check_mode)
    };

    let result = match response_strategy {
        FirstPing => lookup_ip_first_ping(server, &name, options, speed_check_mode).await,
        FastestIp => lookup_ip_fastest_ip(server, &name, options, speed_check_mode).await,
        FastestResponse => lookup_ip_fastest_response(server, &name, options).await,
    };

    handle_strategy_result(result)
}

/// FirstPing 策略: ping 最快 IP 优先，无 ping 结果时回退到多数 IP
async fn lookup_ip_first_ping(
    server: &NameServerGroup,
    name: &Name,
    options: &LookupIpOptions,
    speed_check_mode: &[SpeedCheckMode],
) -> IpStrategyResult {
    // 统一事件流：Ping 和 Query 合并到同一个 FuturesUnordered
    enum Event {
        Ping(Option<IpAddr>),
        Query(Box<Result<DnsResponse, LookupError>>),
    }

    let mut events: FuturesUnordered<BoxFuture<'_, Event>> = server
        .iter()
        .map(|ns| {
            async {
                Event::Query(Box::new(
                    per_nameserver_lookup_ip(ns, name.clone(), options).await,
                ))
            }
            .boxed()
        })
        .collect();

    let mut ok_tasks: Vec<DnsResponse> = vec![];
    let mut err_tasks: Vec<DnsError> = vec![];
    let mut fastest_ip: Option<IpAddr> = None;

    while let Some(event) = events.next().await {
        match event {
            Event::Ping(ip) => {
                fastest_ip = ip;
                break; // ping 一旦返回，立即结束
            }
            Event::Query(res) => match *res {
                Ok(lookup) => {
                    let ip_addrs = lookup.ip_addrs();
                    if ip_addrs.len() == 1 {
                        return IpStrategyResult::DirectResponse(Box::new(lookup));
                    }
                    ok_tasks.push(lookup);
                    // 该 nameserver 返回多个 IP，发起 ping 测速
                    events.push(
                        async move {
                            Event::Ping(
                                multi_mode_ping_fastest(
                                    name.clone(),
                                    ip_addrs,
                                    speed_check_mode.to_vec(),
                                )
                                .await,
                            )
                        }
                        .boxed(),
                    );
                }
                Err(err) => {
                    err_tasks.push(err);
                }
            },
        }
    }

    if let Some(ip) = fastest_ip {
        IpStrategyResult::SelectedIp(ip, ok_tasks, err_tasks)
    } else {
        // 无 ping 结果，选出现次数最多的 IP
        let ip = most_frequent_ip(&ok_tasks);
        match ip {
            Some(ip) => IpStrategyResult::SelectedIp(ip, ok_tasks, err_tasks),
            None => IpStrategyResult::Fallback(ok_tasks, err_tasks),
        }
    }
}

/// FastestIp 策略: 所有查询完成后选 ping 最快的 IP
async fn lookup_ip_fastest_ip(
    server: &NameServerGroup,
    name: &Name,
    options: &LookupIpOptions,
    speed_check_mode: &[SpeedCheckMode],
) -> IpStrategyResult {
    enum Event {
        Ping(Result<PingOutput, PingError>),
        Query(Box<Result<DnsResponse, LookupError>>),
    }

    let mut events: FuturesUnordered<BoxFuture<'_, Event>> = server
        .iter()
        .map(|ns| {
            async {
                Event::Query(Box::new(
                    per_nameserver_lookup_ip(ns, name.clone(), options).await,
                ))
            }
            .boxed()
        })
        .collect();

    let mut ok_tasks: Vec<DnsResponse> = vec![];
    let mut err_tasks: Vec<DnsError> = vec![];
    let mut ip_addr_stats: HashMap<IpAddr, u8> = HashMap::new();
    let mut fastest_ip: Option<PingOutput> = None;

    while let Some(event) = events.next().await {
        match event {
            Event::Ping(res) => {
                if let Ok(out) = res
                    && match fastest_ip.as_ref() {
                        Some(t) => out.elapsed() < t.elapsed(),
                        None => true,
                    }
                {
                    fastest_ip = Some(out);
                }
            }
            Event::Query(res) => match *res {
                Ok(lookup) => {
                    let ip_addrs = lookup.ip_addrs();
                    for &ip_addr in &ip_addrs {
                        *ip_addr_stats.entry(ip_addr).or_insert_with(|| {
                            events.push(
                                async move {
                                    Event::Ping(
                                        multi_mode_ping(
                                            name.clone(),
                                            ip_addr,
                                            speed_check_mode.to_vec(),
                                        )
                                        .await,
                                    )
                                }
                                .boxed(),
                            );
                            0u8
                        }) += 1;
                    }
                    ok_tasks.push(lookup);
                }
                Err(err) => {
                    err_tasks.push(err);
                }
            },
        }
    }

    if let Some(fastest) = fastest_ip {
        IpStrategyResult::SelectedIp(fastest.dest().ip_addr(), ok_tasks, err_tasks)
    } else {
        let ip = most_frequent_ip(&ok_tasks);
        match ip {
            Some(ip) => IpStrategyResult::SelectedIp(ip, ok_tasks, err_tasks),
            None => IpStrategyResult::Fallback(ok_tasks, err_tasks),
        }
    }
}

/// FastestResponse 策略: 返回第一个 NoError 响应，全失败则返回第一个错误
async fn lookup_ip_fastest_response(
    server: &NameServerGroup,
    name: &Name,
    options: &LookupIpOptions,
) -> IpStrategyResult {
    let mut events: FuturesUnordered<BoxFuture<'_, Result<DnsResponse, LookupError>>> = server
        .iter()
        .map(|ns| async { per_nameserver_lookup_ip(ns, name.clone(), options).await }.boxed())
        .collect();

    let mut last_error = None;

    while let Some(res) = events.next().await {
        match &res {
            Ok(response) if response.response_code() == ResponseCode::NoError => {
                return IpStrategyResult::DirectResponse(Box::new(res.unwrap()));
            }
            _ => {}
        }

        if events.is_empty() {
            return match res {
                Ok(r) => IpStrategyResult::DirectResponse(Box::new(r)),
                Err(e) => IpStrategyResult::Error(e),
            };
        }

        if let Err(ref err) = res {
            if matches!(last_error, Some(ref e) if e == err) {
                return IpStrategyResult::Error(res.unwrap_err());
            }
            last_error = Some(err.clone());
        }
    }

    match last_error {
        Some(err) => IpStrategyResult::Error(err),
        None => IpStrategyResult::Error(ProtoErrorKind::NoConnections.into()),
    }
}

/// 从 DNS 响应列表中找出出现次数最多的 IP 地址
fn most_frequent_ip(ok_tasks: &[DnsResponse]) -> Option<IpAddr> {
    ok_tasks
        .iter()
        .flat_map(|r| r.ip_addrs())
        .fold(HashMap::<IpAddr, usize>::new(), |mut map, ip| {
            map.entry(ip).and_modify(|n| *n += 1).or_insert(1);
            map
        })
        .into_iter()
        .max_by_key(|(_, n)| *n)
        .map(|(ip, _)| ip)
}

async fn multi_mode_ping_fastest(
    name: Name,
    ip_addrs: Vec<IpAddr>,
    modes: Vec<SpeedCheckMode>,
) -> Option<IpAddr> {
    use crate::infra::ping::{PingOptions, ping_fastest};
    let duration = Duration::from_millis(200);
    let ping_ops = PingOptions::default().with_timeout_secs(2);

    let mut fastest_ip = None;

    for mode in &modes {
        debug!("Speed test {} {:?} ping {:?}", name, mode, ip_addrs);
        let dests = mode.to_ping_addrs(&ip_addrs);

        let ping_task = ping_fastest(dests, ping_ops).boxed();
        let timeout_task = sleep(duration).boxed();
        match futures_util::future::select(ping_task, timeout_task).await {
            futures::future::Either::Left((ping_res, _)) => {
                match ping_res {
                    Ok(ping_out) => {
                        // ping success
                        let ip = ping_out.dest().ip_addr();
                        debug!(
                            "The fastest ip of {} is {}, delay: {:?}",
                            name,
                            ip,
                            ping_out.elapsed()
                        );
                        fastest_ip = Some(ip);
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

async fn multi_mode_ping(
    name: Name,
    ip_addr: IpAddr,
    modes: Vec<SpeedCheckMode>,
) -> Result<PingOutput, PingError> {
    use crate::infra::ping::{PingOptions, ping};
    let duration = Duration::from_millis(200);
    let ping_ops = PingOptions::default().with_timeout_secs(2);

    for mode in &modes {
        let dest = match mode.to_ping_addr(ip_addr) {
            Some(addr) => addr,
            None => return Err(PingError::NoAddress),
        };

        let ping_task = ping(dest, ping_ops).boxed();
        let timeout_task = sleep(duration).boxed();
        match futures_util::future::select(ping_task, timeout_task).await {
            futures::future::Either::Left((ping_res, _)) => match ping_res {
                Ok(ping_out) => {
                    debug!(
                        "Speed test {} {:?} ping {:?} elapsed {:?}",
                        name,
                        mode,
                        ip_addr,
                        ping_out.elapsed()
                    );
                    return Ok(ping_out);
                }
                Err(_) => continue,
            },
            futures::future::Either::Right((_, _)) => {
                // timeout
                continue;
            }
        }
    }

    Err(PingError::Timeout)
}

async fn per_nameserver_lookup_ip(
    server: &NameServer,
    name: Name,
    options: &LookupIpOptions,
) -> Result<DnsResponse, LookupError> {
    assert!(options.lookup_options.record_type.is_ip_addr());

    let res = server.lookup(name.clone(), options).await;

    let ns_opts = server.options();
    let whitelist_on = ns_opts.whitelist_ip;
    let blacklist_on = ns_opts.blacklist_ip;

    let LookupIpOptions {
        whitelist_ip,
        blacklist_ip,
        ip_alias,
        ignore_ip,
        ..
    } = options;

    if !whitelist_on && !blacklist_on && ignore_ip.is_empty() && ip_alias.is_empty() {
        return res;
    }

    let ip_filter = |ip: &IpAddr| {
        // whitelist
        if whitelist_on && whitelist_ip.contains(ip) {
            return true;
        }

        if blacklist_on && blacklist_ip.contains(ip) {
            return false;
        }

        !ignore_ip.contains(ip)
    };

    match res {
        Ok(mut lookup) => {
            let query = lookup.query().clone();

            let answers = lookup.take_answers();
            let answers = {
                let mut new_ans = Vec::new();
                let mut alias_set = Vec::new(); // dedup
                for record in answers {
                    let Some(ip) = record.data().ip_addr().filter(ip_filter) else {
                        continue;
                    };
                    match ip_alias.get(&ip) {
                        None => new_ans.push(record),
                        Some(ip) if !alias_set.contains(&ip.as_ptr()) => {
                            alias_set.push(ip.as_ptr());
                            new_ans.extend(ip.iter().filter_map(|&ip| {
                                let mut record = record.clone();
                                record.set_data(ip.into());
                                match (options.record_type, ip) {
                                    (RecordType::A, IpAddr::V4(_))
                                    | (RecordType::AAAA, IpAddr::V6(_)) => Some(record),
                                    _ => {
                                        lookup.add_additional(record);
                                        None
                                    }
                                }
                            }));
                        }
                        Some(_) => continue,
                    }
                }
                new_ans
            };

            if answers.is_empty() {
                let no_records = AuthorityData::new(Box::new(query), None, true, true, None);
                return Err(ProtoErrorKind::NoRecordsFound(no_records.into()).into());
            }

            *lookup.answers_mut() = answers;
            lookup.set_valid_until_max();

            Ok(lookup)
        }
        Err(err) => Err(err),
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use crate::libdns::proto::rr::rdata::opt::ClientSubnet;

    use super::*;
    use crate::{dns_conf::RuntimeConfig, third_ext::FutureJoinAllExt};

    #[test]
    fn test_edns_client_subnet() {
        async fn inner_test(i: usize) -> bool {
            // https://lite.ip2location.com/ip-address-ranges-by-country

            let servers = [
                "server https://120.53.53.53/dns-query",
                "server https://223.5.5.5/dns-query",
            ];

            let server = servers[i % servers.len()];

            let cfg = RuntimeConfig::builder().with(server).build().unwrap();

            let domain = "www.bing.com";

            let client = cfg.create_dns_client().await;

            let subnets = ["113.65.29.0/24", "103.225.87.0/24", "113.65.29.0/24"];

            let results = subnets
                .into_iter()
                .map(|subnet| {
                    client.lookup(
                        domain,
                        LookupOptions {
                            is_dnssec: false,
                            record_type: RecordType::A,
                            client_subnet: Some(ClientSubnet::from_str(subnet).unwrap()),
                        },
                    )
                })
                .join_all()
                .await
                .into_iter()
                .flatten()
                .map(|lookup| {
                    let mut ips = lookup.ip_addrs();
                    ips.sort();
                    ips
                })
                .collect::<Vec<_>>();

            let t1 = results[0].clone();
            let t2 = results[1].clone();
            let t3 = results[2].clone();
            let success = t1 == t3 && t1 != t2;
            if !success {
                println!("{t1:?}");
                println!("{t2:?}");
                println!("{t3:?}");
            }
            success
        }

        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async {
                use futures_util::future::select_all;
                let mut success = false;
                let mut tasks = (0..10).map(|i| inner_test(i).boxed()).collect::<Vec<_>>();

                loop {
                    let (res, _idx, rest) = select_all(tasks).await;

                    if res {
                        success = res;
                        break;
                    }

                    if rest.is_empty() {
                        break;
                    }

                    tasks = rest;
                }
                assert!(success);
            });
    }
}
