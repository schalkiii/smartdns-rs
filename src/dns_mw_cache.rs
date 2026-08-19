use chrono::DateTime;
use chrono::Local;
use serde::{Deserialize, Serialize};
use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::collections::hash_map::DefaultHasher;
use std::fs::File;
use std::hash::{Hash, Hasher};
use std::io::Read;
use std::num::NonZeroUsize;
use std::ops::Deref;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use std::time::Instant;

use crate::config::ServerOpts;
use crate::dns_conf::RuntimeConfig;
use crate::libdns::proto::ProtoError;
use crate::log;
use crate::server::DnsHandle;
use crate::{
    dns::*,
    libdns::proto::{
        op::{Message, Query, ResponseCode},
        rr::DNSClass,
    },
    log::{debug, error, info, trace},
    middleware::*,
};
use lru::LruCache;
use tokio::sync::Notify;
use tokio::sync::{Mutex, mpsc};

#[derive(Clone)]
pub struct PrefetchTask {
    pub query: Query,
    pub rule_group: Option<String>,
}

pub struct DnsCacheMiddleware {
    cfg: Arc<RuntimeConfig>,
    cache: Arc<DnsCache>,
    prefetch_notify: Arc<DomainPrefetchingNotify>,
    client: DnsHandle,
    prefetch_sender: Option<mpsc::Sender<PrefetchTask>>,
}

impl DnsCacheMiddleware {
    pub fn new(cfg: &Arc<RuntimeConfig>, dns_handle: DnsHandle) -> Self {
        let cache = Arc::new(DnsCache::new(
            cfg.cache_size(),
            cfg.serve_expired(),
            cfg.serve_expired_ttl(),
            cfg.serve_expired_reply_ttl(),
        ));

        if cfg.cache_persist() {
            let cache_file = cfg.cache_file();
            let cache_clone = cache.clone();
            let cache_checkpoint_time = cfg.cache_checkpoint_time();
            tokio::spawn(async move {
                if cache_file.exists() {
                    cache_clone.load_from(cache_file.as_path()).await;
                }
                let interval = Duration::from_secs(cache_checkpoint_time);
                loop {
                    tokio::select! {
                        _ = tokio::time::sleep(interval) => {
                            let entries = cache_clone.snapshot_entries().await;
                            let cache_file = cache_file.clone();
                            tokio::task::spawn_blocking(move || {
                                let cache_to_file = || {
                                    let mut file = File::options()
                                        .create(true)
                                        .truncate(true)
                                        .write(true)
                                        .open(&cache_file)?;
                                    DnsCacheEntry::serialize_many(entries.iter(), &mut file)
                                };

                                match cache_to_file() {
                                    Ok(_) => log::info!("save DNS cache to file {:?} successfully.", cache_file),
                                    Err(err) => log::error!("failed to save DNS cache to file {}: {}", cache_file.display(), err),
                                }
                            });
                        }
                        _ = crate::signal::terminate() => {
                            cache_clone.persist_to(cache_file.as_path()).await;
                            log::debug!("save DNS cache to file {}", cache_file.display());
                            break;
                        }
                    };
                }
            });
        }

        let client = dns_handle.with_new_opt(ServerOpts {
            is_background: true,
            ..Default::default()
        });

        let prefetch_sender = if cfg.prefetch_domain() {
            // 增加 channel 容量从 32 到 256，防止高负载时 prefetch 任务被丢弃
            let (sender, receiver) = mpsc::channel(256);
            let worker_client = client.clone();
            tokio::spawn(async move {
                prefetch_worker(receiver, worker_client).await;
            });
            Some(sender)
        } else {
            None
        };

        let mw = Self {
            cfg: cfg.clone(),
            cache,
            prefetch_notify: Arc::new(DomainPrefetchingNotify::new()),
            client,
            prefetch_sender,
        };

        if cfg.prefetch_domain() {
            mw.start_prefetching();
        }

        mw
    }

    pub fn cache(&self) -> &Arc<DnsCache> {
        &self.cache
    }

    fn start_prefetching(&self) {
        let prefetch_notify = self.prefetch_notify.clone();
        let sender = self.prefetch_sender.clone();
        let cache = self.cache.clone();
        tokio::spawn(async move {
            let min_interval = Duration::from_millis(
                std::env::var("PREFETCH_MIN_INTERVAL")
                    .as_deref()
                    .unwrap_or("500")
                    .parse()
                    .unwrap_or(500),
            );
            let max_prefetch = std::env::var("PREFETCH_MAX_BATCH")
                .as_deref()
                .unwrap_or("5")
                .parse::<usize>()
                .unwrap_or(5);
            // 每次检查最多取 16 条过期记录，防止启动时洪水般涌入 channel
            const PREFETCH_BATCH_SIZE: usize = 16;
            let mut last_check = Instant::now();
            // 下一次检查的绝对时刻；由扫描循环依据堆中最近过期时间重置。
            // 使用单次可重置定时器（sleep_until），避免原 notify_after 每次插入派生 detached
            // sleep 任务导致定时器随运行时间无界累积（性能劣化根因）。
            let mut next_check = tokio::time::Instant::now();

            loop {
                // 等待：到达预定检查时刻，或被插入路径立即唤醒。
                tokio::select! {
                    _ = tokio::time::sleep_until(next_check) => {}
                    _ = prefetch_notify.notified() => {}
                }

                let now = Instant::now();
                let most_recent;
                if now - last_check > min_interval {
                    last_check = now;

                    let (expired, most_recent0) = cache
                        .get_expired(
                            now,
                            Some(max_prefetch as u64),
                            min_interval,
                            PREFETCH_BATCH_SIZE,
                        )
                        .await;

                    most_recent = most_recent0;

                    if !expired.is_empty()
                        && let Some(sender) = sender.as_ref()
                    {
                        for (query, group) in expired {
                            let query_name = query.name().to_string();
                            // 使用 send 而非 try_send，提供背压，防止 channel 溢出
                            match sender
                                .send(PrefetchTask {
                                    query,
                                    rule_group: group,
                                })
                                .await
                            {
                                Ok(_) => trace!("[prefetch] queued: {}", query_name),
                                Err(_) => {
                                    error!("[prefetch] channel closed");
                                    break;
                                }
                            }
                        }
                    }
                } else {
                    most_recent = Duration::ZERO;
                }

                // 调度下一次检查：堆中最近过期间隔与最小间隔的较大者，单次可重置定时器。
                let dura = most_recent.max(min_interval);
                next_check = tokio::time::Instant::now() + dura;
            }
        });
    }

    /// 尝试将查询加入预取队列，用于后台刷新缓存
    fn try_prefetch(&self, query: &Query, server_group_name: &str) {
        if let Some(sender) = self.prefetch_sender.as_ref() {
            let query_name = query.name().to_string();
            match sender.try_send(PrefetchTask {
                query: query.clone(),
                rule_group: Some(server_group_name.to_string()),
            }) {
                Ok(_) => trace!("[cache] bg-refresh queued: {}", query_name),
                Err(mpsc::error::TrySendError::Full(_)) => {
                    trace!("[cache] queue full, drop bg-refresh: {}", query_name);
                }
                Err(mpsc::error::TrySendError::Closed(_)) => {
                    error!("[cache] prefetch channel closed");
                }
            }
        }
    }
}

pub async fn prefetch_worker(mut receiver: mpsc::Receiver<PrefetchTask>, client: DnsHandle) {
    use std::sync::Arc;
    use tokio::sync::Semaphore;

    debug!("Prefetch worker started");
    // 限制并发 prefetch 查询数为 8，防止资源耗尽同时保证处理速度
    let semaphore = Arc::new(Semaphore::new(8));

    let mut join_set = tokio::task::JoinSet::new();

    loop {
        tokio::select! {
            task = receiver.recv() => {
                let Some(task) = task else {
                    break;
                };
                let now = Instant::now();
                let opts = ServerOpts {
                    is_background: true,
                    rule_group: task.rule_group,
                    ..Default::default()
                };
                let client = client.with_new_opt(opts);
                let query_msg: SerialMessage = task.query.clone().into();
                let qname = task.query.name().to_string();
                let qtype = task.query.query_type();
                let permit = semaphore.clone();

                // 快速取出 channel 中的任务，spawn 后通过 semaphore 控制并发
                join_set.spawn(async move {
                    let _permit = permit.acquire_owned().await;
                    let _ = client.send(query_msg).await;
                    debug!(
                        "[prefetch] {} {} completed, elapsed {:?}",
                        qname,
                        qtype,
                        now.elapsed()
                    );
                });
            }
            Some(_) = join_set.join_next(), if !join_set.is_empty() => {
                // 清理已完成的任务
            }
        }
    }

    // 等待所有正在进行的 prefetch 任务完成
    while (join_set.join_next().await).is_some() {}

    debug!("Prefetch worker stopped");
}

#[async_trait::async_trait]
impl Middleware<DnsContext, DnsRequest, DnsResponse, DnsError> for DnsCacheMiddleware {
    async fn handle(
        &self,
        ctx: &mut DnsContext,
        req: &DnsRequest,
        next: Next<'_, DnsContext, DnsRequest, DnsResponse, DnsError>,
    ) -> Result<DnsResponse, DnsError> {
        // skip cache
        if ctx.server_opts.no_cache() || ctx.no_cache || req.is_dnssec() {
            return next.run(ctx, req).await;
        }

        let query = req.query().original().to_owned();

        let cached_res = if ctx.server_opts.is_background {
            // for background quering, we don't use cache
            None
        } else {
            let no_serve_expired = ctx
                .domain_rule
                .get(|r| r.no_serve_expired)
                .unwrap_or_default();

            let cached_res = self.cache.get(&query, Instant::now()).await;

            match cached_res {
                // check if it's the same nameserver group.
                Some((mut res, status))
                    if res.name_server_group() == Some(ctx.server_group_name()) =>
                {
                    match status {
                        CacheStatus::Valid => {
                            // 否定缓存条目无需主动刷新（不进入 prefetch 就绪堆）。
                            // 仅对剩余 TTL 偏低的条目做命中即预取，避免每个命中都打上游
                            // （堆驱动预取已覆盖到期刷新，详见 ON_HIT_PREFETCH_REMAINING_TTL）。
                            if !is_negative_response(&res, &query)
                                && res.max_ttl().unwrap_or(0) < ON_HIT_PREFETCH_REMAINING_TTL
                            {
                                self.try_prefetch(&query, ctx.server_group_name());
                            }

                            trace!(
                                "[cache] hit: {} {} (valid)",
                                query.name(),
                                query.query_type()
                            );

                            ctx.source = LookupFrom::Cache;
                            res.mark_from_cache();
                            self.cache
                                .query_hits
                                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                            return Ok(res);
                        }
                        CacheStatus::Expired if ctx.cfg().serve_expired() && !no_serve_expired => {
                            // 否定缓存条目无需主动刷新（不进入 prefetch 就绪堆）。
                            // serve-stale 条目 TTL 已被改写为过期回复 TTL（远低于门槛），
                            // 此处一定通过门槛，照常触发后台刷新。
                            if !is_negative_response(&res, &query)
                                && res.max_ttl().unwrap_or(0) < ON_HIT_PREFETCH_REMAINING_TTL
                            {
                                self.try_prefetch(&query, ctx.server_group_name());
                            }

                            trace!(
                                "[cache] hit: {} {} (expired, serve-stale)",
                                query.name(),
                                query.query_type()
                            );
                            ctx.source = LookupFrom::Cache;
                            res.mark_from_cache();
                            self.cache
                                .query_hits
                                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                            return Ok(res);
                        }
                        _ => Some(res),
                    }
                }
                _ => None,
            }
        };

        let res = next.run(ctx, req).await;

        match res {
            Ok(lookup) => {
                // 否定响应（NXDOMAIN / NODATA）缓存：这类响应在家庭网络中占比很高
                // （尤其是双栈域名的 AAAA 查询，常常无 AAAA 记录而返回否定应答），
                // 原版 smartdns 默认缓存之，可显著提升缓存命中率。
                if !ctx.no_cache
                    && ctx.cfg().cache_negative()
                    && is_negative_response(&lookup, &query)
                {
                    let neg_ttl = negative_ttl(&lookup);
                    self.cache
                        .insert_negative(
                            query.clone(),
                            lookup.clone(),
                            neg_ttl,
                            ctx.server_group_name(),
                            Instant::now(),
                        )
                        .await;
                    return Ok(lookup);
                }

                if lookup
                    .records()
                    .iter()
                    .all(|record| record.record_type() != query.query_type())
                {
                    // bypass cache when none of the answer records match the query type
                    // example case:
                    // ;; QUESTION SECTION:
                    // ;secure.sndcdn.com.             IN      AAAA
                    // ;; ANSWER SECTION:
                    // secure.sndcdn.com.      7194    IN      CNAME   d10rxg6s8apbfh.cloudfront.net.
                    // ;; AUTHORITY SECTION:
                    // d10rxg6s8apbfh.cloudfront.net. 54 IN    SOA     ns-1776.awsdns-30.co.uk. awsdns-hostmaster.amazon.com. 1 7200 900 1209600 86400
                    //
                    // the AAAA request resolves to a CNAME, which in turn resolves to an
                    // SOA record, which means no AAAA records where found, but the cache
                    // only stores records from the answer section, so the SOA in the
                    // the authority section is lost, leaving a broken response in cache
                    return Ok(lookup);
                }

                if !ctx.no_cache {
                    let query = req.query().original().to_owned();
                    let server_group_name = ctx.server_group_name();

                    self.cache
                        .insert_records(
                            query,
                            lookup.records().iter().cloned(),
                            Instant::now(),
                            server_group_name,
                        )
                        .await;

                    if ctx.cfg().prefetch_domain() {
                        // 仅唤醒预取扫描循环；实际调度由扫描循环依据堆中最近过期时间完成。
                        // 避免原 notify_after 在每次未命中插入时派生 detached sleep 任务，
                        // 长期运行后累积成千上万个永不取消的定时器（性能劣化根因）。
                        self.prefetch_notify.notify_one();
                    }
                }
                Ok(lookup)
            }
            Err(err) => {
                // 否定响应缓存（NODATA / NXDOMAIN）：
                // 上游"无记录"以 Err(NoRecordsFound) 形式上浮，否定响应承载在错误中而非 Ok 分支，
                // 因此必须在错误分支构造并缓存，否则第 2 次查询仍会打到上游且无法计为缓存命中。
                if !ctx.no_cache
                    && ctx.cfg().cache_negative()
                    && let Some(neg_resp) = negative_response_from_error(&err, &query)
                {
                    let neg_ttl = negative_ttl(&neg_resp);
                    self.cache
                        .insert_negative(
                            query.clone(),
                            neg_resp.clone(),
                            neg_ttl,
                            ctx.server_group_name(),
                            Instant::now(),
                        )
                        .await;
                    // 注意：首次命中（上游已查、刚写入否定缓存）不计为缓存命中，
                    // 也不标记 from_cache —— 与正向缓存一致（只有第 2 次从缓存返回才计命中）。
                    return Ok(neg_resp);
                }
                // fallback to expired result.
                if let Some(res) = cached_res {
                    return Ok(res);
                }
                Err(err)
            }
        }
    }
}

/// 判断响应是否为"否定响应"，可被安全地缓存一段时长。
///
/// - `NXDOMAIN`：域名不存在。
/// - `NODATA`：域名存在但不含所查询类型的记录。典型场景是双栈域名的 AAAA 查询，
///   上游常返回 `CNAME` + `SOA`（无 AAAA 记录），应答区**非空**（含 CNAME），
///   但没有任何 AAAA 记录——这正是 RFC 2308 定义的 NODATA。因此不能以"应答区为空"
///   判定，而应以"应答区不含查询类型的记录"判定，否则这类高频响应会被漏缓存，
///   导致命中率显著低于原版 smartdns。
///
/// `SERVFAIL` / `REFUSED` 等错误响应不缓存，避免掩盖上游故障。
fn is_negative_response(resp: &DnsResponse, query: &Query) -> bool {
    match resp.response_code() {
        ResponseCode::NXDomain => true,
        ResponseCode::NoError => !resp
            .answers()
            .iter()
            .any(|r| r.record_type() == query.query_type()),
        _ => false,
    }
}

/// 从 SOA 记录的 minimum 字段（RFC 2308 否定缓存 TTL）提取否定缓存时长。
/// 无 SOA 时回退到 600s，并做合理限幅（下限 60s，上限 3600s）。
fn negative_ttl(resp: &DnsResponse) -> Duration {
    let mut ttl = 600u64;
    for auth in resp.authorities() {
        if let RData::SOA(soa) = auth.data() {
            ttl = u64::from(soa.minimum()).max(60);
            break;
        }
    }
    Duration::from_secs(ttl.min(3600))
}

/// 从否定错误（NXDOMAIN / NODATA）构造可缓存的否定 `DnsResponse`。
///
/// 与 `app.rs` 最终的错误→响应转换逻辑保持一致：
/// - `NXDOMAIN`：返回 `NXDOMAIN` 响应码（域名不存在）。
/// - `NODATA` 且携带 SOA：返回 `NOERROR` + SOA 权威段。
/// - 其余（无 SOA 的 referral / 普通失败）：返回 `None`，不缓存
///   （RFC 2308 指出无 SOA 的否定响应不应缓存，避免否定响应无限循环）。
fn negative_response_from_error(err: &DnsError, query: &Query) -> Option<DnsResponse> {
    if err.is_nx_domain() {
        let mut res = DnsResponse::new_with_max_ttl(query.to_owned(), Vec::new());
        res.set_response_code(ResponseCode::NXDomain);
        return Some(res);
    }
    err.as_soa(query)
}

/// 轻量预取唤醒器：仅用于立即唤醒预取扫描循环，不持有定时器、不派生任务。
///
/// 原实现 `notify_after` 在每次调用时 `tokio::spawn` 一个 detached `sleep` 任务，
/// 长期运行后缓存未命中写入会累积成千上万个永不取消的定时器 future，
/// 既占用 tokio timer 堆内存，又频繁误唤醒扫描循环（持锁全表扫描），导致性能劣化。
///
/// 新设计：
/// - 插入路径只调用 [`DomainPrefetchingNotify::notify_one`] 立即唤醒扫描循环，零任务派生；
/// - 扫描循环自行依据堆中最近过期时间用 `sleep_until` 调度下一次检查（单个可重置定时器），
///   彻底消除定时器泄漏。
struct DomainPrefetchingNotify {
    notity: Arc<Notify>,
}

impl DomainPrefetchingNotify {
    pub fn new() -> Self {
        Self {
            notity: Default::default(),
        }
    }

    /// 立即唤醒预取扫描循环（无定时器、无任务派生）。
    pub fn notify_one(&self) {
        self.notity.notify_one();
    }
}

impl Deref for DomainPrefetchingNotify {
    type Target = Notify;

    fn deref(&self) -> &Self::Target {
        self.notity.as_ref()
    }
}

/// Maximum TTL as defined in https://tools.ietf.org/html/rfc2181, 2147483647
/// Setting this to a value of 1 day, in seconds
const MAX_TTL: u32 = 86400_u32;

/// An LRU eviction cache specifically for storing DNS records
/// 缓存分片数。将单一全局锁拆分为 N 个分片锁，降低高 QPS 下的串行化天花板。
const CACHE_SHARD_COUNT: usize = 16;

/// 命中即预取（on-hit prefetch）的剩余 TTL 门槛（秒）。
///
/// 仅在缓存条目剩余 TTL 低于该值时，才在缓存命中时触发后台刷新。
/// 堆驱动预取（`get_expired`）已在条目到期时刷新，命中即预取只是补充机制；
/// 设门槛可避免对每一次命中（尤其是长 TTL 的热域名）都向上游发查询，
/// 显著削减后台预取查询量，同时不影响"临近过期即刷新"的保鲜效果。
const ON_HIT_PREFETCH_REMAINING_TTL: u32 = 600;

/// prefetch 就绪堆的键：仅按过期时间 `Instant` 排序（Query 不参与比较，
/// 因为 hickory 的 Query 未实现 Ord）。用于 BinaryHeap<Reverse<...>> 形成最小堆。
#[derive(Clone)]
struct PrefetchKey(Instant, Query);

impl PartialEq for PrefetchKey {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}
impl Eq for PrefetchKey {}
impl PartialOrd for PrefetchKey {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for PrefetchKey {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0.cmp(&other.0)
    }
}

/// 根据 query 稳定地映射到分片索引。insert/get/get_expired 必须保持一致。
#[inline]
fn shard_index_of(query: &Query, n: usize) -> usize {
    let mut h = DefaultHasher::new();
    query.name().hash(&mut h);
    query.query_type().hash(&mut h);
    query.query_class().hash(&mut h);
    (h.finish() as usize) % n
}

pub struct DnsCache {
    /// 分片缓存：每个分片一把独立的 tokio Mutex，路由由 shard_index_of 决定。
    shards: Vec<Arc<Mutex<LruCache<Query, DnsCacheEntry>>>>,
    /// prefetch 就绪堆：按过期时间排序的最小堆。insert 时压入，
    /// get_expired 时只弹出已过期的条目，避免对整张 LruCache 做全表扫描。
    prefetch_heap: Mutex<BinaryHeap<Reverse<PrefetchKey>>>,
    serve_expired: bool,
    expired_ttl: u64,
    expired_reply_ttl: u64,
    query_hits: std::sync::atomic::AtomicU64,
}

impl DnsCache {
    fn new(
        cache_size: usize,
        serve_expired: bool,
        expired_ttl: u64,
        expired_reply_ttl: u64,
    ) -> Self {
        // 选择分片数：当配置容量小于分片数时退化为单分片，
        // 避免每片容量被钳制到 1 后同片键互相驱逐（命中率下降 / 单测失败）。
        let shard_count = if cache_size >= CACHE_SHARD_COUNT {
            CACHE_SHARD_COUNT
        } else {
            1
        };
        // 将总容量尽量均摊到各分片，并把余数分摊到前 `cache_size % shard_count` 个分片，
        // 保证各分片容量 >= 1 且总容量 >= cache_size。
        let per_shard = (cache_size / shard_count).max(1);
        let remainder = cache_size.saturating_sub(per_shard * shard_count);
        let shards = (0..shard_count)
            .map(|i| {
                let cap = per_shard + if i < remainder { 1 } else { 0 };
                Arc::new(Mutex::new(LruCache::new(NonZeroUsize::new(cap).unwrap())))
            })
            .collect();

        Self {
            shards,
            prefetch_heap: Mutex::new(BinaryHeap::new()),
            serve_expired,
            expired_ttl,
            expired_reply_ttl,
            query_hits: std::sync::atomic::AtomicU64::new(0),
        }
    }

    /// 缓存条目总数（跨所有分片求和），用于日志、调试与统计接口。
    pub async fn entry_count(&self) -> usize {
        let mut total = 0;
        for shard in &self.shards {
            total += shard.lock().await.len();
        }
        total
    }

    /// 导出全部缓存条目快照（用于周期落盘）。
    async fn snapshot_entries(&self) -> Vec<DnsCacheEntry> {
        let mut out = Vec::new();
        for shard in &self.shards {
            let guard = shard.lock().await;
            for (_, entry) in guard.iter() {
                out.push(entry.clone());
            }
        }
        out
    }

    async fn persist_to<P: AsRef<Path>>(&self, path: P) {
        let entries = self.snapshot_entries().await;
        let path = path.as_ref().to_path_buf();
        // 落盘为阻塞 IO，放到 spawn_blocking 避免阻塞异步运行时。
        let path_in_closure = path.clone();
        if let Err(err) = tokio::task::spawn_blocking(move || {
            let mut file = File::options()
                .create(true)
                .truncate(true)
                .write(true)
                .open(&path_in_closure)?;
            DnsCacheEntry::serialize_many(entries.iter(), &mut file)
        })
        .await
        .unwrap_or_else(|e| {
            Err(ProtoError::from(std::io::Error::other(format!(
                "join error: {e}"
            ))))
        }) {
            error!("failed to save DNS cache file {}: {}", path.display(), err);
        } else {
            info!("save DNS cache to file {:?} successfully.", path);
        }
    }

    async fn load_from<P: AsRef<Path>>(&self, path: P) {
        let path = path.as_ref().to_path_buf();
        let path_in_closure = path.clone();
        let data = match tokio::task::spawn_blocking(move || {
            let mut file = File::options().read(true).open(&path_in_closure)?;
            let mut data = vec![];
            file.read_to_end(&mut data)?;
            Ok::<_, std::io::Error>(data)
        })
        .await
        {
            Ok(Ok(data)) => data,
            Ok(Err(e)) => {
                error!("failed to read DNS cache file {}: {}", path.display(), e);
                return;
            }
            Err(e) => {
                error!(
                    "failed to read DNS cache file {}: join error {e}",
                    path.display()
                );
                return;
            }
        };

        let now = Instant::now();
        match DnsCacheEntry::deserialize_many(&data) {
            Ok(entries) => {
                let count = entries.len();
                for entry in entries {
                    let query = entry.data.query().clone();
                    let idx = shard_index_of(&query, self.shards.len());
                    self.shards[idx]
                        .lock()
                        .await
                        .put(query.clone(), entry.clone());
                    // 重建就绪堆（用条目的真实 valid_until）。
                    self.prefetch_heap
                        .lock()
                        .await
                        .push(Reverse(PrefetchKey(entry.valid_until, query)));
                }
                info!(
                    "DNS cache {} records loaded, elapsed {:?}",
                    count,
                    now.elapsed()
                );
            }
            Err(err) => error!("failed to read DNS cache file {:?} {}", path, err),
        }
    }

    pub async fn clear(&self) {
        for shard in &self.shards {
            shard.lock().await.clear();
        }
        // 同步清空预取就绪堆，否则堆中残留旧 Query 会在下次 get_expired 时才被回收。
        self.prefetch_heap.lock().await.clear();
    }

    pub async fn cached_records(&self) -> Vec<CachedQueryRecord> {
        let mut out = Vec::new();
        for shard in &self.shards {
            let guard = shard.lock().await;
            for (query, entry) in guard.iter() {
                out.push(CachedQueryRecord {
                    name: query.name().clone(),
                    query_type: query.query_type(),
                    query_class: query.query_class(),
                    records: entry.data.records().to_vec().into_boxed_slice(),
                    hits: entry.stats.hits,
                    last_access: entry.stats.last_access,
                });
            }
        }
        out
    }

    pub fn query_hits(&self) -> u64 {
        self.query_hits.load(std::sync::atomic::Ordering::Relaxed)
    }

    async fn insert(
        &self,
        query: Query,
        records_and_ttl: Vec<(Record, u32)>,
        now: Instant,
        name_server_group: &str,
    ) -> DnsResponse {
        let len = records_and_ttl.len();
        // collapse the values, we're going to take the Minimum TTL as the correct one
        let (records, ttl): (Vec<Record>, Duration) = records_and_ttl.into_iter().fold(
            (Vec::with_capacity(len), Duration::from_secs(600)),
            |(mut records, mut min_ttl), (record, ttl)| {
                records.push(record);
                let ttl = Duration::from_secs(u64::from(ttl));
                min_ttl = min_ttl.min(ttl);
                (records, min_ttl)
            },
        );

        let valid_until = now + ttl;

        let lookup = DnsResponse::new_with_deadline(query.clone(), records, valid_until)
            .with_name_server_group(name_server_group.to_string());

        let idx = shard_index_of(&query, self.shards.len());
        {
            let mut shard = self.shards[idx].lock().await;
            if let Some(entry) = shard.get_mut(&query) {
                entry.data = lookup.clone();
                entry.valid_until = valid_until;
                entry.stats.hit();
            } else {
                shard.put(
                    query.clone(),
                    DnsCacheEntry::new(lookup.clone(), valid_until),
                );
            }
        }
        // 压入 prefetch 就绪堆（最小堆按过期时间排序）。
        self.prefetch_heap
            .lock()
            .await
            .push(Reverse(PrefetchKey(valid_until, query)));

        lookup
    }

    /// 插入否定响应（NXDOMAIN / NODATA）。
    ///
    /// 否定响应与正向响应共用同一张表：其 `DnsResponse` 已携带正确的响应码
    /// （NXDOMAIN）与 SOA，命中时由 `get` 原样返回即可，客户端收到否定应答，
    /// 同时本次查询计入缓存命中。否定条目不进入 prefetch 就绪堆（无需主动刷新）。
    async fn insert_negative(
        &self,
        query: Query,
        mut resp: DnsResponse,
        ttl: Duration,
        name_server_group: &str,
        now: Instant,
    ) {
        let valid_until = now + ttl;
        resp = resp.with_name_server_group(name_server_group.to_string());
        // 将应答区记录的 TTL 限幅到否定 TTL 范围内，避免向下游返回过大的 TTL。
        resp.set_max_ttl(ttl.as_secs() as u32);

        let idx = shard_index_of(&query, self.shards.len());
        let mut shard = self.shards[idx].lock().await;
        if let Some(entry) = shard.get_mut(&query) {
            entry.data = resp.clone();
            entry.valid_until = valid_until;
            entry.stats.hit();
        } else {
            shard.put(query.clone(), DnsCacheEntry::new(resp.clone(), valid_until));
        }
    }

    /// inserts a record based on the name and type.
    ///
    /// # Arguments
    ///
    /// * `original_query` - is used for matching the records that should be returned
    /// * `records` - the records will be partitioned by type and name for storage in the cache
    /// * `now` - current time for use in associating TTLs
    ///
    /// # Return
    ///
    /// This should always return some records, but will be None if there are no records or the original_query matches none
    async fn insert_records(
        &self,
        original_query: Query,
        records: impl Iterator<Item = Record>,
        now: Instant,
        name_server_group: &str,
    ) -> Option<DnsResponse> {
        let mut is_cname_query = false;
        // collect all records by name
        let records = records.fold(
            Vec::<(Query, Vec<(Record, u32)>)>::new(),
            |mut map, record| {
                let mut query = Query::query(record.name().clone(), record.record_type());
                query.set_query_class(record.dns_class());

                let ttl = record.ttl();

                if original_query != query {
                    is_cname_query = true;
                }

                let val = (record, ttl);
                match map.iter_mut().find(|e| e.0 == query) {
                    Some(entry) => entry.1.push(val),
                    None => map.push((query, vec![val])),
                }

                map
            },
        );

        // now insert by record type and name
        let mut lookup = None;

        if is_cname_query {
            let records = records
                .clone()
                .into_iter()
                .flat_map(|(_, r)| r)
                .collect::<Vec<_>>();

            lookup = Some(
                self.insert(original_query.clone(), records, now, name_server_group)
                    .await,
            )
        }

        for (query, records_and_ttl) in records {
            let is_query = original_query == query;
            let inserted = self
                .insert(query, records_and_ttl, now, name_server_group)
                .await;

            if is_query {
                lookup = Some(inserted)
            }
        }

        lookup
    }

    /// This converts the ResolveError to set the inner negative_ttl value to be the
    ///  current expiration ttl.
    fn nx_error_with_ttl(_error: &mut DnsError, _new_ttl: Duration) {
        // if let ResolveError {
        //     kind:
        //         ResolveErrorKind::NoRecordsFound {
        //             ref mut negative_ttl,
        //             ..
        //         },
        //     ..
        // } = error
        // {
        //     *negative_ttl = Some(u32::try_from(new_ttl.as_secs()).unwrap_or(MAX_TTL));
        // }
    }

    /// Based on the query, see if there are any records available
    async fn get(&self, query: &Query, now: Instant) -> Option<(DnsResponse, CacheStatus)> {
        let idx = shard_index_of(query, self.shards.len());
        let mut guard = self.shards[idx].lock().await;

        guard.get_mut(query).map(|value| {
            value.stats.hit();
            let mut res = value.data.clone();

            // For CNAME query, the cached response might only contain A/AAAA records
            // with the final name of the CNAME chain. If so, we should rewrite
            // the record names to match the original query name.
            // We detect this by checking if there are no CNAME records in the
            // response, all records are IP records, and there are records with a
            // name different from the query name.
            let has_cname = res
                .answers()
                .iter()
                .any(|r| r.record_type() == RecordType::CNAME);

            let all_ip_records = !res.answers().is_empty()
                && res.answers().iter().all(|r| r.record_type().is_ip_addr());

            if !has_cname
                && all_ip_records
                && res.answers().iter().any(|r| r.name() != query.name())
            {
                let query_name = query.name().clone();
                for record in res.answers_mut() {
                    record.set_name(query_name.clone());
                }
            }

            if value.is_current(now) {
                res.set_max_ttl(value.ttl(now).as_secs() as u32);
                (res, CacheStatus::Valid)
            } else {
                res.set_max_ttl(self.expired_reply_ttl as u32);
                (res, CacheStatus::Expired)
            }
        })
    }

    async fn get_expired(
        &self,
        now: Instant,
        seconds_ahead: Option<u64>,
        base_interval: Duration,
        max_count: usize,
    ) -> (Vec<(Query, Option<String>)>, Duration) {
        // 关键：绝不“持堆锁跨分片锁 await”。原实现在持有 prefetch_heap 锁的整个扫描期
        // 内逐个 await 分片锁，而 insert 路径是“分片→堆”顺序，二者形成锁顺序反转，
        // 并发下会死锁（预取 worker 与缓存写入相互等待、双向挂起）。
        // 这里改为：每次只从堆弹出一个候选、立即释放堆锁，再单独去锁分片判定；
        // 需要保留在堆中的条目稍后统一放回。
        let mut expired = Vec::with_capacity(max_count);
        let mut requeue: Vec<PrefetchKey> = Vec::new();

        // 判定过期的阈值：考虑 expired_ttl 提前量与 seconds_ahead 提前量。
        let threshold = if self.expired_ttl > 0 {
            now.checked_sub(Duration::from_secs(self.expired_ttl))
                .unwrap_or(now)
        } else {
            now
        } + Duration::from_secs(seconds_ahead.unwrap_or(5));

        loop {
            // 取出堆顶（最早过期）候选并立即释放堆锁，避免与 insert 的锁顺序相撞。
            let top = {
                let mut heap = self.prefetch_heap.lock().await;
                match heap.pop() {
                    Some(t) => t.0,
                    None => break,
                }
            };
            let PrefetchKey(exp, query) = top;

            // 尚未到过期阈值：放回堆顶并停止（最小堆，后面必然更晚）。
            if exp > threshold {
                requeue.push(PrefetchKey(exp, query));
                break;
            }
            // 已达批量上限：放回并停止，下次再取。
            if expired.len() >= max_count {
                requeue.push(PrefetchKey(exp, query));
                break;
            }

            let idx = shard_index_of(&query, self.shards.len());
            let mut guard = self.shards[idx].lock().await;
            let entry = match guard.get_mut(&query) {
                Some(e) => e,
                None => continue, // 已被 LRU 淘汰，堆中存在过期条目，丢弃
            };
            if !entry.should_retry_prefetch(threshold, base_interval) {
                continue;
            }
            if !query.query_type().is_ip_addr() {
                continue;
            }
            if entry.is_current(threshold) {
                // 已被刷新，尚未真正过期：用最新 valid_until 放回堆，待下次再判。
                requeue.push(PrefetchKey(entry.valid_until, query));
                continue;
            }

            entry.is_in_prefetching = true;

            expired.push((
                query.to_owned(),
                entry.stats.hits,
                entry.data.name_server_group().map(String::from),
            ));
        }

        // 把需要保留的堆条目放回（释放堆锁期间被弹出的那些）。
        if !requeue.is_empty() {
            let mut heap = self.prefetch_heap.lock().await;
            for key in requeue {
                heap.push(Reverse(key));
            }
        }

        // 下次检查时间：取堆中下一个（最早的）未过期条目距离现在的间隔。
        let most_recent = {
            let heap = self.prefetch_heap.lock().await;
            match heap.peek().cloned() {
                Some(Reverse(PrefetchKey(exp, _))) if exp > threshold => exp
                    .saturating_duration_since(threshold)
                    .min(Duration::from_secs(MAX_TTL as u64)),
                _ => Duration::from_secs(MAX_TTL as u64),
            }
        };

        expired.sort_by_key(|(_, hits, _)| std::cmp::Reverse(*hits));

        (
            expired.into_iter().map(|(q, _, g)| (q, g)).collect(),
            most_recent,
        )
    }
}

#[derive(Debug, Clone, Copy)]
enum CacheStatus {
    Valid,
    Expired,
}

#[derive(Deserialize, Serialize)]
pub struct CachedQueryRecord {
    pub name: Name,
    pub hits: usize,
    pub last_access: DateTime<Local>,
    pub query_type: RecordType,
    pub query_class: DNSClass,
    pub records: Box<[Record]>,
}

#[derive(Clone)]
struct DnsCacheEntry<T = DnsResponse> {
    data: T,
    valid_until: Instant,
    is_in_prefetching: bool,
    prefetch_failure_time: Option<Instant>,
    stats: DnsCacheStats,
}

impl<T> DnsCacheEntry<T> {
    fn new(data: T, valid_until: Instant) -> Self {
        Self {
            data,
            valid_until,
            is_in_prefetching: false,
            prefetch_failure_time: None,
            stats: DnsCacheStats::new(),
        }
    }

    fn set_data(&mut self, data: T) {
        self.data = data;
        self.is_in_prefetching = false;
        self.prefetch_failure_time = None;
    }

    fn set_valid_until(&mut self, valid_until: Instant) {
        self.valid_until = valid_until;
    }

    fn is_current(&self, now: Instant) -> bool {
        now <= self.valid_until
    }

    fn ttl(&self, now: Instant) -> Duration {
        self.valid_until.saturating_duration_since(now)
    }

    fn should_retry_prefetch(&self, now: Instant, base_interval: Duration) -> bool {
        if !self.is_in_prefetching {
            return true;
        }
        if let Some(failure_time) = self.prefetch_failure_time
            && now >= failure_time + base_interval * 2
        {
            return true;
        }
        false
    }
}

#[derive(Clone)]
struct DnsCacheStats {
    /// The number of lookups that have been performed
    hits: usize,
    last_access: DateTime<Local>,
}

impl DnsCacheStats {
    fn new() -> Self {
        Self {
            hits: 0,
            last_access: Local::now(),
        }
    }

    fn hit(&mut self) {
        self.hits += 1;
        self.last_access = Local::now();
    }
}

use crate::libdns::proto::serialize::binary::{
    BinDecodable, BinDecoder, BinEncodable, BinEncoder, DecodeError,
};

impl BinEncodable for DnsCacheEntry<DnsResponse> {
    fn emit(&self, encoder: &mut BinEncoder<'_>) -> Result<(), ProtoError> {
        let res = &self.data;

        // message
        encoder.emit_u8(1)?;
        res.deref().emit(encoder)?;

        // valid_until
        encoder.emit_u8(2)?;
        let now = Instant::now();
        let ttl = if self.valid_until > now {
            self.valid_until - now
        } else {
            Duration::ZERO
        };
        encoder.emit_u32(ttl.as_secs() as u32)?;

        // group_name
        encoder.emit_u8(3)?;
        if let Some(group_name) = res.name_server_group().map(|n| n.as_bytes()) {
            encoder.emit_u16(group_name.len() as u16)?;
            encoder.emit_vec(group_name)?;
        } else {
            encoder.emit_u16(0)?;
        }

        // hits
        encoder.emit_u8(4)?;
        encoder.emit_u32(self.stats.hits as u32)?;
        Ok(())
    }
}

impl<'r> BinDecodable<'r> for DnsCacheEntry {
    fn read(decoder: &mut BinDecoder<'r>) -> Result<Self, ProtoError> {
        // message
        if !decoder.read_u8()?.verify(|v| *v == 1).is_valid() {
            return Err(DecodeError::InsufficientBytes.into());
        }
        let message = Message::read(decoder)?;

        // valid_until
        if !decoder.read_u8()?.verify(|v| *v == 2).is_valid() {
            return Err(DecodeError::InsufficientBytes.into());
        }
        let ttl_secs = decoder.read_u32()?.unverified();
        let valid_until = Instant::now() + Duration::from_secs(ttl_secs as u64);

        // group_name
        if !decoder.read_u8()?.verify(|v| *v == 3).is_valid() {
            return Err(DecodeError::InsufficientBytes.into());
        }
        let group_name = {
            let name_len = decoder.read_u16()?.unverified();
            if name_len > 0 {
                let name_bytes = decoder.read_slice(name_len as usize)?.unverified();
                String::from_utf8(name_bytes.to_vec()).ok()
            } else {
                None
            }
        };

        // hits
        if !decoder.read_u8()?.verify(|v| *v == 4).is_valid() {
            return Err(DecodeError::InsufficientBytes.into());
        }
        let hits = decoder.read_u32()?.unverified();

        // construct the response
        let mut res: DnsResponse = message.into();
        res = res.with_valid_until(valid_until);
        if let Some(g) = group_name {
            res = res.with_name_server_group(g);
        }
        let mut entry = DnsCacheEntry::new(res, valid_until);
        entry.stats.hits = hits as usize;

        Ok(entry)
    }
}

impl DnsCacheEntry {
    fn serialize_many<'a>(
        entries: impl Iterator<Item = &'a DnsCacheEntry>,
        writer: &mut impl std::io::Write,
    ) -> Result<(), ProtoError> {
        let mut buf = vec![];

        for entry in entries {
            buf.clear();
            let mut encoder = BinEncoder::new(&mut buf);
            if (*entry).emit(&mut encoder).is_ok() {
                let _ = writer.write_all(&buf);
            }
        }
        Ok(())
    }

    fn deserialize_many(data: &[u8]) -> Result<Vec<DnsCacheEntry>, ProtoError> {
        let mut entries = vec![];
        let mut offset = 0;

        while offset < data.len() {
            let mut decoder = BinDecoder::new(&data[offset..]);
            entries.push(DnsCacheEntry::read(&mut decoder)?);
            offset += decoder.index();
        }

        Ok(entries)
    }
}

trait PersistCache {
    fn persist<P: AsRef<Path>>(&self, path: P);

    fn load<P: AsRef<Path>>(&mut self, path: P);
}

impl PersistCache for LruCache<Query, DnsCacheEntry> {
    fn persist<P: AsRef<Path>>(&self, path: P) {
        let path = path.as_ref();
        let cache_to_file = || {
            let mut file = File::options()
                .create(true)
                .truncate(true)
                .write(true)
                .open(path)?;
            let entries = self.iter().map(|(_, entry)| entry);
            DnsCacheEntry::serialize_many(entries, &mut file)
        };

        match cache_to_file() {
            Ok(_) => info!("save DNS cache to file {:?} successfully.", path),
            Err(err) => error!("failed to save DNS cache to file {}", err),
        }
    }

    fn load<P: AsRef<Path>>(&mut self, path: P) {
        let path = path.as_ref();
        info!("reading DNS cache from file: {:?}", path);
        let now = Instant::now();

        let read_from_cache_file = || {
            let mut file = File::options().read(true).open(path)?;
            let mut data = vec![];
            file.read_to_end(&mut data)?;

            DnsCacheEntry::deserialize_many(&data)
        };

        match read_from_cache_file() {
            Ok(entries) => {
                let count = entries.len();
                let cache = self;
                for entry in entries {
                    let query = entry.data.query().clone();
                    cache.put(query, entry);
                }
                info!(
                    "DNS cache {} records loaded, elapsed {:?}",
                    count,
                    now.elapsed()
                );
            }
            Err(err) => error!("failed to read DNS cache file {:?} {}", path, err),
        }
    }
}

#[cfg(test)]
mod tests {

    use rr::rdata::{A, CNAME};

    use super::*;

    fn create_lookup(name: &str, data: RData, ttl: u64) -> DnsCacheEntry {
        let name: Name = name.parse().unwrap();
        let ttl = Duration::from_secs(ttl);
        let query = Query::query(name.clone(), data.record_type());
        let records = vec![Record::from_rdata(name, ttl.as_secs() as u32, data)];
        let valid_until = Instant::now() + ttl;
        DnsCacheEntry::new(
            DnsResponse::new_with_deadline(query, records, valid_until),
            valid_until,
        )
    }

    #[test]
    fn test_lookup_serde() {
        let lookups = [
            create_lookup(
                "abc.exmample.com.",
                RData::A("127.0.0.1".parse().unwrap()),
                30,
            ),
            create_lookup("xyz.exmample.com.", RData::AAAA("::1".parse().unwrap()), 38),
        ];

        let mut data = vec![];
        DnsCacheEntry::serialize_many(lookups.iter(), &mut data).unwrap();
        let lookup2 = DnsCacheEntry::deserialize_many(&data).unwrap();

        assert_eq!(lookup2.len(), lookups.len());

        assert_eq!(&lookups[0].data, &lookup2[0].data);
        assert_eq!(&lookups[1].data, &lookup2[1].data);
    }

    #[test]
    fn test_is_negative_response_cname_nodata() {
        let name: Name = "www.example.com.".parse().unwrap();
        let aaaa_q = Query::query(name.clone(), RecordType::AAAA);
        let cname_q = Query::query(name.clone(), RecordType::CNAME);

        // AAAA 查询，应答仅含一条 CNAME（无 AAAA）→ 典型双栈 NODATA，应判为否定。
        // 修复前因 is_negative_response 要求"应答区为空"，漏判此类高频响应，导致命中率偏低。
        let cname_target: Name = "target.example.com.".parse().unwrap();
        let cname = Record::from_rdata(name.clone(), 60, RData::CNAME(CNAME(cname_target)));
        let mut resp = DnsResponse::new_with_max_ttl(aaaa_q.clone(), vec![cname]);
        resp.set_response_code(ResponseCode::NoError);
        assert!(
            is_negative_response(&resp, &aaaa_q),
            "AAAA 查询仅返回 CNAME 应判为 NODATA 否定响应"
        );

        // 同一应答，若查询类型恰为 CNAME，则存在匹配记录，不是否定
        assert!(
            !is_negative_response(&resp, &cname_q),
            "CNAME 查询命中 CNAME 应答，不应判为否定"
        );

        // AAAA 查询且应答含 AAAA → 不是否定
        let aaaa = Record::from_rdata(name.clone(), 60, RData::AAAA("::1".parse().unwrap()));
        let mut resp_aaaa = DnsResponse::new_with_max_ttl(aaaa_q.clone(), vec![aaaa]);
        resp_aaaa.set_response_code(ResponseCode::NoError);
        assert!(
            !is_negative_response(&resp_aaaa, &aaaa_q),
            "AAAA 应答不应判为否定"
        );

        // 空应答 NODATA
        let mut resp_empty = DnsResponse::new_with_max_ttl(aaaa_q.clone(), vec![]);
        resp_empty.set_response_code(ResponseCode::NoError);
        assert!(
            is_negative_response(&resp_empty, &aaaa_q),
            "空应答 NODATA 应判为否定"
        );

        // NXDOMAIN
        let mut resp_nx = DnsResponse::new_with_max_ttl(aaaa_q.clone(), vec![]);
        resp_nx.set_response_code(ResponseCode::NXDomain);
        assert!(
            is_negative_response(&resp_nx, &aaaa_q),
            "NXDOMAIN 应判为否定"
        );
    }

    #[tokio::test]
    async fn test_cache_persist() {
        let lookup1 = create_lookup(
            "abc.exmample.com.",
            RData::A("127.0.0.1".parse().unwrap()),
            3000,
        );
        let lookup2 = create_lookup(
            "xyz.exmample.com.",
            RData::AAAA("::1".parse().unwrap()),
            3000,
        );

        let cache = DnsCache::new(10, true, 30, 5);

        let now = Instant::now();

        cache
            .insert_records(
                lookup1.data.query().clone(),
                lookup1.data.record_iter().cloned(),
                now,
                "default",
            )
            .await;

        cache
            .insert_records(
                lookup2.data.query().clone(),
                lookup2.data.record_iter().cloned(),
                now,
                "default",
            )
            .await;

        tokio::time::sleep(Duration::from_millis(500)).await;

        assert!(cache.get(lookup1.data.query(), now).await.is_some());

        assert_eq!(cache.entry_count().await, 2);

        cache.persist_to("./logs/smartdns-test.cache").await;
        assert!(cache.get(lookup1.data.query(), now).await.is_some());

        cache.clear().await;
        assert_eq!(cache.entry_count().await, 0);

        cache.load_from("./logs/smartdns-test.cache").await;
        assert_eq!(cache.entry_count().await, 2);

        assert!(cache.get(lookup1.data.query(), now).await.is_some());
        assert!(cache.get(lookup2.data.query(), now).await.is_some());

        let res = cache.get(lookup1.data.query(), now).await;

        assert!(res.is_some());

        let (lookup, _) = res.unwrap();

        assert_eq!(lookup.query(), lookup1.data.query());
        assert_eq!(lookup.records(), lookup1.data.records());
    }

    #[tokio::test]
    async fn test_cache_record_ordering() {
        let query = Query::query("www.vscode-unpkg.net.".parse().unwrap(), RecordType::A);
        let records = [
            Record::from_rdata(
                "www.vscode-unpkg.net.".parse().unwrap(),
                2028,
                RData::CNAME(CNAME(
                    "vscode-unpkg-gvgaavacadd3anb4.z01.azurefd.net."
                        .parse()
                        .unwrap(),
                )),
            ),
            Record::from_rdata(
                "vscode-unpkg-gvgaavacadd3anb4.z01.azurefd.net."
                    .parse()
                    .unwrap(),
                2,
                RData::CNAME(CNAME(
                    "star-azurefd-prod.trafficmanager.net.".parse().unwrap(),
                )),
            ),
            Record::from_rdata(
                "star-azurefd-prod.trafficmanager.net.".parse().unwrap(),
                32,
                RData::CNAME(CNAME(
                    "shed.dual-low.s-part-0031.t-0009.t-msedge.net."
                        .parse()
                        .unwrap(),
                )),
            ),
            Record::from_rdata(
                "shed.dual-low.s-part-0031.t-0009.t-msedge.net."
                    .parse()
                    .unwrap(),
                32,
                RData::CNAME(CNAME("s-part-0031.t-0009.t-msedge.net.".parse().unwrap())),
            ),
            Record::from_rdata(
                "s-part-0031.t-0009.t-msedge.net.".parse().unwrap(),
                32,
                RData::A(A("13.107.246.59".parse().unwrap())),
            ),
        ];

        let cache = DnsCache::new(10, true, 30, 5);

        let now = Instant::now();

        cache
            .insert_records(query.clone(), records.iter().cloned(), now, "default")
            .await;

        tokio::task::yield_now().await;

        assert!(cache.get(&query, now).await.unwrap().0.records() == records);
    }

    #[tokio::test]
    async fn test_prefetch_task_queue_full() {
        let (tx, mut rx) = mpsc::channel::<PrefetchTask>(2);

        let query = Query::query("test.example.com.".parse().unwrap(), RecordType::A);
        let task = PrefetchTask {
            query,
            rule_group: None,
        };

        assert!(tx.try_send(task.clone()).is_ok());
        assert!(tx.try_send(task.clone()).is_ok());

        let full_result = tx.try_send(task);
        assert!(full_result.is_err());
        assert!(matches!(
            full_result,
            Err(mpsc::error::TrySendError::Full(_))
        ));

        drop(tx);
        let count = rx.recv().await;
        assert!(count.is_some());
    }

    #[tokio::test]
    async fn test_prefetch_worker_processes_tasks() {
        let (mut request_rx, handle) = DnsHandle::new();

        let (tx, rx) = mpsc::channel(32);

        let worker = tokio::spawn(prefetch_worker(rx, handle));

        let query = Query::query("example.com.".parse().unwrap(), RecordType::A);
        let task = PrefetchTask {
            query,
            rule_group: Some("test_group".to_string()),
        };

        tx.send(task).await.unwrap();
        drop(tx);

        let worker_result = tokio::spawn(async move {
            let mut count = 0;
            while let Some((_msg, opts, reply_tx)) = request_rx.recv().await {
                assert_eq!(opts.rule_group.as_deref(), Some("test_group"));
                let response = Message::query().to_response();
                let _ = reply_tx.send(SerialMessage::from(response));
                count += 1;
                if count >= 1 {
                    break;
                }
            }
            count
        });

        let (worker_ok, processed) = tokio::join!(worker, worker_result);
        assert!(worker_ok.is_ok());
        assert_eq!(processed.unwrap(), 1);
    }

    #[tokio::test]
    async fn test_prefetch_worker_batch_processing() {
        let (mut request_rx, handle) = DnsHandle::new();
        let (tx, rx) = mpsc::channel(32);

        let worker = tokio::spawn(prefetch_worker(rx, handle));

        let domains = vec!["a.com", "b.com", "c.com"];
        for domain in domains {
            let query = Query::query(format!("{}.", domain).parse().unwrap(), RecordType::A);
            let task = PrefetchTask {
                query,
                rule_group: None,
            };
            tx.send(task).await.unwrap();
        }

        drop(tx);

        let worker_result = tokio::spawn(async move {
            let mut count = 0;
            while let Some((_msg, _opts, reply_tx)) = request_rx.recv().await {
                let response = Message::query().to_response();
                let _ = reply_tx.send(SerialMessage::from(response));
                count += 1;
                if count >= 3 {
                    break;
                }
            }
            count
        });

        let (worker_ok, processed) = tokio::join!(worker, worker_result);
        assert!(worker_ok.is_ok());
        assert_eq!(processed.unwrap(), 3);
    }

    #[tokio::test]
    async fn test_prefetch_worker_stops_on_channel_close() {
        let (tx, rx) = mpsc::channel::<PrefetchTask>(32);

        let handle = tokio::spawn(async move {
            let mut receiver = rx;
            let mut count = 0;
            while let Some(_task) = receiver.recv().await {
                count += 1;
            }
            count
        });

        drop(tx);

        let count = handle.await.unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn test_prefetch_task_public_fields() {
        let query = Query::query("test.com.".parse().unwrap(), RecordType::AAAA);
        let task = PrefetchTask {
            query: query.clone(),
            rule_group: Some("group1".to_string()),
        };
        assert_eq!(task.query.name().to_string(), "test.com.");
        assert_eq!(task.rule_group.as_deref(), Some("group1"));
    }
}
