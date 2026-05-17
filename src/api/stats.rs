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
    cache_hits: usize,
    total_queries: u64,
    cache_hit_rate: f64,
    avg_query_time_ms: f64,
    version: &'static str,
}

#[get("/stats", tag = "Stats")]
async fn stats(State(state): State<Arc<ServeState>>) -> Json<DnsStats> {
    let app = &state.app;
    let (cache_size, cache_hits) = if let Some(c) = app.cache().await {
        let records = c.cached_records().await;
        let size = records.len();
        let hits: usize = records.iter().map(|r| r.hits).sum();
        (size, hits)
    } else {
        (0, 0)
    };

    let total_queries = app.total_queries();
    let cache_hit_rate = app.cache_hit_rate(cache_hits as u64);
    let avg_query_time_ms = app.avg_query_time_ms();

    Json(DnsStats {
        uptime_secs: app.uptime().as_secs(),
        active_queries: app.active_queries(),
        cache_size,
        cache_hits,
        total_queries,
        cache_hit_rate,
        avg_query_time_ms,
        version: crate::BUILD_VERSION,
    })
}