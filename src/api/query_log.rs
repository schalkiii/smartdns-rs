use std::sync::Arc;

use axum::{Json, extract::Query, extract::State};
use serde::Serialize;

use super::openapi::{IntoRouter, http::get, routes};
use super::{ServeState, StatefulRouter};

pub fn routes() -> StatefulRouter {
    routes![query_log].into_router()
}

/// 查询日志列表（内存环形缓冲，最近 N 条）。
#[derive(Serialize)]
pub struct QueryLogList {
    /// 过滤后的总条数
    pub total: usize,
    /// 当前页数据（最新在前）
    pub data: Vec<crate::dns_mw_audit::QueryLogEntry>,
}

#[derive(serde::Deserialize)]
pub struct QueryLogParams {
    pub limit: Option<usize>,
    pub offset: Option<usize>,
    pub domain: Option<String>,
    pub client: Option<String>,
    pub qtype: Option<String>,
}

#[get("/query-log")]
async fn query_log(
    State(state): State<Arc<ServeState>>,
    Query(params): Query<QueryLogParams>,
) -> Json<QueryLogList> {
    // 复制环形缓冲内容（最新在尾部，反转到最新在前）。
    let entries: Vec<crate::dns_mw_audit::QueryLogEntry> = {
        let ring = state.app.query_log();
        let q = ring.lock().unwrap();
        q.iter().rev().cloned().collect()
    };

    // 按条件过滤（大小写不敏感）。
    let entries: Vec<crate::dns_mw_audit::QueryLogEntry> = entries
        .into_iter()
        .filter(|e| {
            if let Some(d) = &params.domain
                && !e.domain.to_lowercase().contains(&d.to_lowercase())
            {
                return false;
            }
            if let Some(c) = &params.client
                && !e.client.to_lowercase().contains(&c.to_lowercase())
            {
                return false;
            }
            if let Some(t) = &params.qtype
                && e.query_type.to_uppercase() != t.to_uppercase()
            {
                return false;
            }
            true
        })
        .collect();

    let total = entries.len();
    let offset = params.offset.unwrap_or(0);
    let limit = params.limit.unwrap_or(100).clamp(1, 1000);
    let data: Vec<crate::dns_mw_audit::QueryLogEntry> =
        entries.into_iter().skip(offset).take(limit).collect();

    Json(QueryLogList { total, data })
}
