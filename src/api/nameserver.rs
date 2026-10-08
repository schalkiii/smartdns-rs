use std::collections::HashMap;
use std::sync::Arc;

use axum::{Json, extract::State};
use serde::Serialize;

use crate::dns_url::ProtocolConfig;

use super::openapi::{IntoRouter, http::get, routes};
use super::{DataListPayload, ServeState, StatefulRouter};

pub fn routes() -> StatefulRouter {
    routes![nameservers].into_router()
}

/// 上游服务器信息（含运行期统计）：用于 WebUI 上游服务器页面。
#[derive(Serialize)]
pub struct UpstreamServer {
    pub name: Option<String>,
    pub server: String,
    pub group: Vec<String>,
    pub protocol: String,
    pub security: String,
    pub ip: String,
    pub port: u16,
    pub status: String,
    pub query_total: u64,
    pub success_total: u64,
    pub failure_total: u64,
    pub avg_time_ms: f64,
    pub success_rate: f64,
}

#[derive(Default)]
struct Agg {
    query: u64,
    success: u64,
    failure: u64,
    time_ns: u64,
    failing: bool,
}

impl Agg {
    fn avg_time_ms(&self) -> f64 {
        if self.query == 0 {
            0.0
        } else {
            self.time_ns as f64 / self.query as f64 / 1_000_000.0
        }
    }
}

fn protocol_label(p: &ProtocolConfig) -> String {
    match p {
        ProtocolConfig::Udp => "UDP",
        ProtocolConfig::Tcp => "TCP",
        ProtocolConfig::System => "System",
        ProtocolConfig::Dhcp { .. } => "DHCP",
        #[cfg(feature = "dns-over-tls")]
        ProtocolConfig::Tls => "DoT",
        #[cfg(feature = "dns-over-https")]
        ProtocolConfig::Https { .. } => "DoH",
        #[cfg(feature = "dns-over-quic")]
        ProtocolConfig::Quic => "DoQ",
        #[cfg(feature = "dns-over-h3")]
        ProtocolConfig::H3 { .. } => "DoH3",
        // 编译期未启用的协议，兜底显示。
        #[allow(unreachable_patterns)]
        _ => "Other",
    }
    .to_string()
}

#[get("/nameservers")]
async fn nameservers(
    State(state): State<Arc<ServeState>>,
) -> Json<DataListPayload<UpstreamServer>> {
    let cfg = state.app.cfg().await;
    let dns_client = state.app.dns_client().await;

    // 汇总所有上游组（默认组 + 命名组）的运行期统计，按键合并（同一上游可能加入多个组）。
    let mut agg: HashMap<String, Agg> = HashMap::new();
    if let Some(dc) = &dns_client {
        let groups = std::iter::once(dc.default_server_group())
            .chain(dc.named_server_groups().values().cloned());
        for group in groups {
            for ns in group.iter() {
                let key = ns.info().stats_key();
                let a = agg.entry(key).or_default();
                a.query += ns.query_count();
                a.success += ns.success_count();
                a.failure += ns.failure_count();
                a.time_ns += ns.total_time_ns();
                if ns.is_failing() {
                    a.failing = true;
                }
            }
        }
    }

    let servers: Vec<UpstreamServer> = cfg
        .servers()
        .iter()
        .filter(|s| s.enabled())
        .map(|info| {
            let key = info.stats_key();
            let (query, success, failure, avg, failing) = match agg.get(&key) {
                Some(a) => (a.query, a.success, a.failure, a.avg_time_ms(), a.failing),
                None => (0, 0, 0, 0.0, false),
            };
            let sp = info.server.proto();
            let protocol = protocol_label(sp);
            let security = if info.server.proto().is_encrypted() {
                "加密".to_string()
            } else {
                "明文".to_string()
            };
            let ip = info
                .server
                .ip()
                .map(|ip| ip.to_string())
                .unwrap_or_else(|| info.server.host().to_string());
            UpstreamServer {
                name: info.name.clone(),
                server: info.server.to_string(),
                group: info.group.clone(),
                protocol,
                security,
                ip,
                port: info.server.port(),
                status: if failing {
                    "error".to_string()
                } else {
                    "ok".to_string()
                },
                query_total: query,
                success_total: success,
                failure_total: failure,
                avg_time_ms: avg,
                success_rate: if query > 0 {
                    success as f64 / query as f64 * 100.0
                } else {
                    0.0
                },
            }
        })
        .collect();

    Json(servers.into())
}
