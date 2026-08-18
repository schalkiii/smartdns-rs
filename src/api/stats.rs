use std::sync::Arc;

use axum::{Json, extract::State};
use serde::Serialize;

use super::openapi::{IntoRouter, ToSchema, http::get, routes};
use super::{ServeState, StatefulRouter};

pub fn routes() -> StatefulRouter {
    routes![stats].into_router()
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
