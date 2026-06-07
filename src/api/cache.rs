use std::sync::Arc;

use super::openapi::{
    IntoRouter,
    http::{get, post},
    routes,
};
use super::{ServeState, StatefulRouter};
use crate::{api::DataListPayload, config::CacheConfig, dns_mw_cache::CachedQueryRecord, log};
use axum::{
    Json,
    extract::{Query, State},
    http::StatusCode,
};
use serde::Deserialize;

pub fn routes() -> StatefulRouter {
    let r1 = routes![flush, caches].into_router();
    let r2 = routes![config].into_router();
    r1.merge(r2)
}

#[derive(Debug, Deserialize)]
struct CacheListParams {
    offset: Option<usize>,
    limit: Option<usize>,
}

#[get("/caches", tag = "Caches", operation_id = "list_caches")]
async fn caches(
    State(state): State<Arc<ServeState>>,
    Query(params): Query<CacheListParams>,
) -> Json<DataListPayload<CachedQueryRecord>> {
    let mut caches = if let Some(c) = state.app.cache().await {
        c.cached_records().await
    } else {
        vec![]
    };

    let total = caches.len();
    let offset = params.offset.unwrap_or(0);
    let limit = params.limit.unwrap_or(50);

    let data = if offset < caches.len() {
        let end = (offset + limit).min(caches.len());
        caches.drain(offset..end).collect()
    } else {
        vec![]
    };

    Json(DataListPayload {
        count: data.len(),
        total,
        data,
    })
}

#[post("/caches/flush", tag = "Caches", operation_id = "flush_caches")]
async fn flush(State(state): State<Arc<ServeState>>) -> StatusCode {
    if let Some(c) = state.app.cache().await {
        c.clear().await;
    }
    log::info!("flushed cache");
    StatusCode::NO_CONTENT
}

#[get("/caches/config", tag = "Caches", operation_id = "get_cache_config")]
async fn config(State(state): State<Arc<ServeState>>) -> Json<CacheConfig> {
    let config = state.app.cfg().await.cache_config().clone();
    Json(config)
}
