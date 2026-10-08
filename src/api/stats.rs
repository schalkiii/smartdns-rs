use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};

use axum::{Json, extract::Query, extract::State};
use serde::Serialize;

use super::openapi::{IntoRouter, ToSchema, http::get, routes};
use super::{ServeState, StatefulRouter};

pub fn routes() -> StatefulRouter {
    let mut router = routes![stats].into_router();
    router = router.merge(routes![top_domains].into_router());
    router = router.merge(routes![clients].into_router());
    router = router.merge(routes![query_types].into_router());
    router
}

#[derive(Debug, Clone, Serialize, ToSchema)]
struct DnsStats {
    uptime_secs: u64,
    active_queries: usize,
    cache_size: usize,
    cache_hits: u64,
    cache_query_hits: u64,
    total_queries: u64,
    bg_total_queries: u64,
    cache_hit_rate: f64,
    avg_query_time_ms: f64,
    cache_hit_avg_query_time_ms: f64,
    cache_miss_avg_query_time_ms: f64,
    cache_hit_queries: u64,
    cache_miss_queries: u64,
    bg_avg_query_time_ms: f64,
    version: &'static str,
    history: Vec<crate::app::StatsSnapshot>,
}

#[get("/stats", tag = "Stats")]
async fn stats(State(state): State<Arc<ServeState>>) -> Json<DnsStats> {
    let app = &state.app;
    // 注：cache_hits / cache_query_hits 均改用 O(1) 的全局命中计数 query_hits，
    // 不再在每次 /stats 轮询时全量克隆整个缓存（cache-size=65536 下会克隆数万条记录）。
    // cache_hits 此前为各条目命中计数之和（跨重启累加、与 total_queries 不可比），
    // 现与 cache_query_hits 统一为"自启动以来的缓存命中总数"，语义一致且零开销。
    let (cache_size, cache_hits, cache_query_hits) = if let Some(c) = app.cache().await {
        let size = c.entry_count().await;
        let query_hits = c.query_hits();
        (size, query_hits, query_hits)
    } else {
        (0, 0, 0)
    };

    let total_queries = app.total_queries();
    let bg_total_queries = app.bg_total_queries();
    let cache_hit_rate = if total_queries > 0 {
        (cache_query_hits as f64 / total_queries as f64 * 100.0).min(100.0)
    } else {
        0.0
    };
    let avg_query_time_ms = app.avg_query_time_ms();
    let cache_hit_avg_query_time_ms = app.cache_hit_avg_query_time_ms();
    let cache_miss_avg_query_time_ms = app.cache_miss_avg_query_time_ms();
    let cache_hit_queries = app.cache_hit_queries();
    let cache_miss_queries = app.cache_miss_queries();
    let bg_avg_query_time_ms = app.bg_avg_query_time_ms();

    app.add_stats_snapshot(cache_query_hits).await;
    let history = app.stats_history().await;

    Json(DnsStats {
        uptime_secs: app.uptime().as_secs(),
        active_queries: app.active_queries(),
        cache_size,
        cache_hits,
        cache_query_hits,
        total_queries,
        bg_total_queries,
        cache_hit_rate,
        avg_query_time_ms,
        cache_hit_avg_query_time_ms,
        cache_miss_avg_query_time_ms,
        cache_hit_queries,
        cache_miss_queries,
        bg_avg_query_time_ms,
        version: crate::BUILD_VERSION,
        history,
    })
}

/// 名称 + 计数（及平均耗时），用于热门域名 / 客户端等排行。
#[derive(Debug, Clone, Serialize, ToSchema)]
struct NameCount {
    name: String,
    count: u64,
    avg_time_ms: f64,
}

#[derive(serde::Deserialize)]
struct RankParams {
    limit: Option<usize>,
}

/// 从 WebUI 查询日志环形缓冲聚合统计（最近 N 条，内存非持久化）。
fn aggregate(
    ring: &Arc<Mutex<VecDeque<crate::dns_mw_audit::QueryLogEntry>>>,
    classify: impl Fn(&crate::dns_mw_audit::QueryLogEntry) -> Option<String>,
) -> Vec<NameCount> {
    let q = ring.lock().unwrap();
    let mut map: HashMap<String, (u64, f64)> = HashMap::new();
    for e in q.iter() {
        if let Some(key) = classify(e) {
            let entry = map.entry(key).or_insert((0, 0.0));
            entry.0 += 1;
            entry.1 += e.elapsed_ms;
        }
    }
    let mut v: Vec<NameCount> = map
        .into_iter()
        .map(|(name, (count, total))| NameCount {
            name,
            count,
            avg_time_ms: if count > 0 { total / count as f64 } else { 0.0 },
        })
        .collect();
    v.sort_by(|a, b| {
        b.count.cmp(&a.count).then(
            b.avg_time_ms
                .partial_cmp(&a.avg_time_ms)
                .unwrap_or(std::cmp::Ordering::Equal),
        )
    });
    v
}

#[get("/stats/top-domains", tag = "Stats")]
async fn top_domains(
    State(state): State<Arc<ServeState>>,
    Query(params): Query<RankParams>,
) -> Json<Vec<NameCount>> {
    let ring = state.app.query_log();
    let mut v = aggregate(&ring, |e| Some(e.domain.clone()));
    let limit = params.limit.unwrap_or(10).clamp(1, 100);
    v.truncate(limit);
    Json(v)
}

#[get("/stats/clients", tag = "Stats")]
async fn clients(
    State(state): State<Arc<ServeState>>,
    Query(params): Query<RankParams>,
) -> Json<Vec<NameCount>> {
    let ring = state.app.query_log();
    let mut v = aggregate(&ring, |e| Some(e.client.clone()));
    let limit = params.limit.unwrap_or(10).clamp(1, 100);
    v.truncate(limit);
    Json(v)
}

#[get("/stats/query-types", tag = "Stats")]
async fn query_types(State(state): State<Arc<ServeState>>) -> Json<Vec<NameCount>> {
    let ring = state.app.query_log();
    let v = aggregate(&ring, |e| Some(e.query_type.clone()));
    Json(v)
}
