use std::collections::VecDeque;
use std::io;
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use std::time::Instant;

use chrono::prelude::*;
use serde::Serialize;
use smallvec::SmallVec;
use tokio::sync::mpsc::{self, Sender};

use crate::dns::*;
use crate::infra::mapped_file::MappedFile;
use crate::libdns::proto::op::Query;
use crate::log::warn;
use crate::middleware::*;

/// 内存中保留的最近查询日志条数（用于 WebUI 查询日志页面展示）。
pub const QUERY_LOG_MAX: usize = 1000;

/// WebUI 查询日志条目（内存环形缓冲，非持久化）。
#[derive(Debug, Clone, Serialize)]
pub struct QueryLogEntry {
    pub time: DateTime<Local>,
    pub client: String,
    pub domain: String,
    pub query_type: String,
    pub elapsed_ms: f64,
    pub result: String,
    pub speed_ms: Option<f64>,
    pub source: String,
    /// address 拦截规则（含 adblock 订阅）命中标记
    pub blocked: bool,
}

impl From<&DnsAuditRecord> for QueryLogEntry {
    fn from(a: &DnsAuditRecord) -> Self {
        QueryLogEntry {
            time: a.date,
            client: a.client.clone(),
            domain: a.query.name().to_utf8(),
            query_type: a.query.query_type().to_string(),
            elapsed_ms: a.elapsed.as_secs_f64() * 1000.0,
            result: if a.result_ok {
                "success".to_string()
            } else {
                "failed".to_string()
            },
            speed_ms: if a.speed.is_zero() {
                None
            } else {
                Some(a.speed.as_secs_f64() * 1000.0)
            },
            source: format!("{:?}", a.lookup_source),
            blocked: a.blocked,
        }
    }
}

pub struct DnsAuditMiddleware {
    audit_sender: Sender<DnsAuditRecord>,
    query_log: Arc<Mutex<VecDeque<QueryLogEntry>>>,
    /// address 拦截规则（含 adblock 订阅）命中计数（与 App.blocked_queries 共享）
    blocked_counter: Arc<AtomicU64>,
    audit_soa: bool,
    audit_console: bool,
    /// 是否存在磁盘/syslog/console 审计消费者：决定是否格式化明细并发送记录。
    audit_output_enabled: bool,
}

#[async_trait::async_trait]
impl Middleware<DnsContext, DnsRequest, DnsResponse, DnsError> for DnsAuditMiddleware {
    async fn handle(
        &self,
        ctx: &mut DnsContext,
        req: &DnsRequest,
        next: Next<'_, DnsContext, DnsRequest, DnsResponse, DnsError>,
    ) -> Result<DnsResponse, DnsError> {
        let now = Local::now();

        let start = Instant::now();

        let res = next.run(ctx, req).await;

        // address 拦截规则（含 adblock 订阅）命中：递增拦截计数，
        // 且拦截的 SOA-only 应答始终写入查询日志（见下方跳过逻辑的例外）。
        let blocked = ctx.blocked;
        if blocked {
            self.blocked_counter.fetch_add(1, Ordering::Relaxed);
        }

        // 对应 C 版 audit.c:127：SOA-only 应答默认不写审计，audit-SOA yes 时才记录。
        // 例外：拦截（blocked）应答始终进 WebUI 查询日志，
        // 否则 Top 拦截域名统计（依赖查询日志聚合）将始终为空。
        if !blocked && !self.audit_soa && is_soa_only(&res) {
            return res;
        }

        let duration = start.elapsed();

        // 仅当存在磁盘/syslog/console 审计消费者时才格式化结果明细，
        // 否则跳过（WebUI 查询日志只需要成功/失败状态），消除每请求的全量 Message 克隆。
        let result_detail = if self.audit_output_enabled {
            Some(fmt_result_detail(&res))
        } else {
            None
        };

        let audit = DnsAuditRecord {
            id: req.id(),
            date: now,
            client: req.src().to_string(),
            query: req.query().original().to_owned(),
            result_ok: res.is_ok(),
            result_detail,
            elapsed: duration,
            speed: ctx.fastest_speed,
            lookup_source: ctx.source.clone(),
            blocked: ctx.blocked,
        };

        // 写入 WebUI 查询日志环形缓冲（内存，非持久化）。
        {
            let mut q = self.query_log.lock().unwrap();
            q.push_back(QueryLogEntry::from(&audit));
            if q.len() > QUERY_LOG_MAX {
                q.pop_front();
            }
        }

        // debug!("{}", audit.to_string_without_date());

        // 无审计消费者时接收端已 drop，发送必然失败：跳过发送，避免每请求一次
        // try_send + warn 日志的开销。
        if self.audit_output_enabled && self.audit_sender.try_send(audit).is_err() {
            warn!("audit queue full, dropping audit record");
        }

        res
    }
}

/// 审计输出配置。
#[derive(Debug, Clone, Default)]
pub struct AuditOptions {
    pub file: Option<PathBuf>,
    pub size: u64,
    pub num: usize,
    pub mode: Option<u32>,
    pub soa: bool,
    pub console: bool,
    pub syslog: bool,
}

impl DnsAuditMiddleware {
    /// 构建审计中间件。
    ///
    /// `query_log` 为 WebUI 查询日志环形缓冲（始终写入，无论是否启用磁盘审计）；
    /// `blocked_counter` 为 address 拦截规则命中计数（与 App.blocked_queries 共享）；
    /// `audit_file` 为可选的磁盘审计文件路径，仅在启用审计文件时传入，否则仅写入内存查询日志。
    pub fn new(
        query_log: Arc<Mutex<VecDeque<QueryLogEntry>>>,
        blocked_counter: Arc<AtomicU64>,
        options: AuditOptions,
    ) -> Self {
        let AuditOptions {
            file: audit_file,
            size: audit_size,
            num: audit_num,
            mode,
            soa: audit_soa,
            console: audit_console,
            syslog: audit_syslog,
        } = options;

        let (audit_tx, mut audit_rx) = mpsc::channel::<DnsAuditRecord>(100);

        // 是否存在审计消费者：
        // - 文件审计（console 输出内嵌在文件消费任务中，依赖文件启用）
        // - syslog（仅 unix 有消费者；非 unix 上接收端被 drop）
        #[cfg(unix)]
        let audit_output_enabled = audit_file.is_some() || audit_syslog;
        #[cfg(not(unix))]
        let audit_output_enabled = audit_file.is_some();

        if let Some(audit_file) = audit_file {
            tokio::spawn(async move {
                let mut audit_file =
                    MappedFile::open(audit_file, audit_size, Some(audit_num), mode);

                const BUF_SIZE: usize = 10;
                let mut buf: SmallVec<[DnsAuditRecord; BUF_SIZE]> = SmallVec::new();

                while let Some(audit) = audit_rx.recv().await {
                    buf.push(audit);
                    if buf.len() == BUF_SIZE {
                        if let Err(err) = record_audit_to_file(&mut audit_file, buf.as_slice()) {
                            warn!("log audit failed {}", err)
                        }
                        if audit_console {
                            for record in buf.iter() {
                                println!("{}", record.to_string_without_date());
                            }
                        }
                        buf.clear();
                    }
                }
            });
        } else if audit_syslog {
            #[cfg(unix)]
            tokio::spawn(async move {
                let formatter = syslog::Formatter3164 {
                    facility: syslog::Facility::LOG_DAEMON,
                    hostname: None,
                    process: "smartdns-audit".to_string(),
                    pid: std::process::id(),
                };
                let mut logger = syslog::unix(formatter).ok();

                while let Some(audit) = audit_rx.recv().await {
                    if let Some(logger) = logger.as_mut() {
                        let _ = logger.info(&audit.to_string_without_date());
                    }
                }
            });
            #[cfg(not(unix))]
            {
                warn!("audit-syslog is only supported on unix");
                drop(audit_rx);
            }
        } else {
            // 无磁盘审计：丢弃文件写入通道，避免通道无限堆积。
            drop(audit_rx);
        }

        Self {
            audit_sender: audit_tx,
            query_log,
            blocked_counter,
            audit_soa,
            audit_console,
            audit_output_enabled,
        }
    }
}

#[derive(Debug, Clone)]
pub struct DnsAuditRecord {
    id: u16,
    client: String,
    query: Query,
    /// 查询是否成功（WebUI 查询日志只需要此信息）。
    result_ok: bool,
    /// 结果明细（"rdata ttl type" 竖线分隔；失败时为 "query failed"）。
    /// 仅在启用磁盘/syslog 审计时格式化，避免热路径上的全量 Message 克隆。
    result_detail: Option<String>,
    speed: Duration,
    elapsed: Duration,
    date: DateTime<Local>,
    lookup_source: LookupFrom,
    /// address 拦截规则（含 adblock 订阅）命中标记
    blocked: bool,
}

impl DnsAuditRecord {
    fn fmt_result(&self) -> &str {
        self.result_detail
            .as_deref()
            .unwrap_or(if self.result_ok { "" } else { "query failed" })
    }

    fn to_string_without_date(&self) -> String {
        format!(
            "{} query {}, type: {}, elapsed: {:?}, speed: {:?}, result {}",
            self.client,
            self.query.name(),
            self.query.query_type(),
            self.elapsed,
            self.speed,
            self.fmt_result()
        )
    }
}

impl std::fmt::Display for DnsAuditRecord {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "[{}] {} query {}, type: {}, elapsed: {:?}, speed: {:?}, result {}",
            self.date.format("%Y-%m-%d %H:%M:%S,%3f"),
            self.client,
            self.query.name(),
            self.query.query_type(),
            self.elapsed,
            self.speed,
            self.fmt_result()
        )
    }
}

/// 将查询结果格式化为 "rdata ttl type" 竖线分隔的明细字符串。
/// 仅在启用审计输出时调用（热路径外）。
fn fmt_result_detail(result: &Result<DnsResponse, DnsError>) -> String {
    if let Ok(lookup) = result.as_ref() {
        let mut out = String::new();

        for (i, record) in lookup
            .records()
            .iter()
            // .filter(|r| r.data().is_some())
            .enumerate()
        {
            let data = record.data();

            if i > 0 {
                out.push('|');
            }

            out.push_str(data.to_string().as_str());

            out.push(' ');
            out.push_str(record.ttl().to_string().as_str());
            out.push(' ');
            out.push_str(record.record_type().to_string().as_str());
        }
        out
    } else {
        "query failed".to_string()
    }
}

/// 判断应答是否为“只有 SOA、没有任何地址记录”的空应答。
fn is_soa_only(result: &Result<DnsResponse, DnsError>) -> bool {
    let Ok(res) = result else {
        return false;
    };

    let mut has_soa = false;
    let mut has_ip = false;
    for record in res.records() {
        match record.record_type() {
            RecordType::SOA => has_soa = true,
            RecordType::A | RecordType::AAAA => has_ip = true,
            _ => {}
        }
    }

    has_soa && !has_ip
}

fn record_audit_to_file(
    audit_file: &mut MappedFile,
    audit_records: &[DnsAuditRecord],
) -> io::Result<()> {
    if matches!(audit_file.extension(), Some(ext) if ext == "csv") {
        // write as csv

        if audit_file.peamble().is_none() {
            let mut writer = csv::Writer::from_writer(vec![]);
            writer.write_record([
                "id",
                "timestamp",
                "client",
                "name",
                "type",
                "elapsed",
                "speed",
                "state",
                "result",
                "lookup_source",
            ])?;

            if let Ok(buf) = writer.into_inner() {
                audit_file.set_peamble(Some(buf.into_boxed_slice()));
            } else {
                warn!("failed to flush csv preamble to audit file");
            }
        }

        let mut writer = csv::Writer::from_writer(audit_file);

        for audit in audit_records {
            writer.write_record([
                audit.id.to_string().as_str(),
                audit.date.timestamp().to_string().as_str(),
                audit.client.as_str(),
                audit.query.name().to_string().as_str(),
                audit.query.query_type().to_string().as_str(),
                format!("{:?}", audit.elapsed).as_str(),
                format!("{:?}", audit.speed).as_str(),
                if audit.result_ok { "success" } else { "failed" },
                audit.fmt_result(),
                format!("{:?}", audit.lookup_source).as_str(),
            ])?;
        }
    } else {
        // write as nornmal log format.
        for audit in audit_records {
            if writeln!(audit_file, "{audit}").is_err() {
                warn!("Write audit to file '{:?}' failed", audit_file.path());
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {

    use crate::libdns::proto::op::Query;
    use crate::libdns::proto::rr::{RData, RecordType};
    use std::io::Read;
    use std::path::Path;
    use std::str::FromStr;

    use super::*;

    #[test]
    fn test_dns_audit_to_string() {
        let now = "2022-11-11 20:18:11.099966887 +08:00".parse().unwrap();
        let query = Query::query(Name::from_str("www.example.com").unwrap(), RecordType::A);
        let result = Ok(DnsResponse::from_rdata(
            query.to_owned(),
            RData::A("93.184.216.34".parse().unwrap()),
        ));

        let audit = DnsAuditRecord {
            id: 11,
            date: now,
            client: "127.0.0.1".to_string(),
            query,
            result_ok: result.is_ok(),
            result_detail: Some(fmt_result_detail(&result)),
            elapsed: Duration::from_millis(10),
            speed: Duration::from_millis(11),
            blocked: false,
            lookup_source: LookupFrom::Server("default".to_string()),
        };

        assert_eq!(
            audit.to_string(),
            format!(
                "[{}] 127.0.0.1 query www.example.com, type: A, elapsed: 10ms, speed: 11ms, result 93.184.216.34 86400 A",
                now.format("%Y-%m-%d %H:%M:%S,%3f")
            )
        );
    }

    #[test]
    fn test_dns_audit_to_string_without_date() {
        let now = "2022-11-11 20:18:11.099966887 +08:00".parse().unwrap();

        let query = Query::query(Name::from_str("www.example.com").unwrap(), RecordType::A);
        let result = Ok(DnsResponse::from_rdata(
            query.to_owned(),
            RData::A("93.184.216.34".parse().unwrap()),
        ));

        let audit = DnsAuditRecord {
            id: 11,
            date: now,
            client: "127.0.0.1".to_string(),
            query,
            result_ok: result.is_ok(),
            result_detail: Some(fmt_result_detail(&result)),
            elapsed: Duration::from_millis(10),
            speed: Duration::from_millis(11),
            blocked: false,
            lookup_source: LookupFrom::Server("default".to_string()),
        };

        assert_eq!(
            audit.to_string_without_date(),
            "127.0.0.1 query www.example.com, type: A, elapsed: 10ms, speed: 11ms, result 93.184.216.34 86400 A"
        );
    }

    #[test]
    fn test_record_audit_to_file() {
        let query = Query::query(Name::from_str("www.example.com").unwrap(), RecordType::A);

        let result = Ok(DnsResponse::from_rdata(
            query.to_owned(),
            RData::A("93.184.216.34".parse().unwrap()),
        ));

        let now = "2022-11-11 20:18:11.099966887 +08:00".parse().unwrap();

        let audit = DnsAuditRecord {
            id: 11,
            date: now,
            client: "127.0.0.1".to_string(),
            query,
            result_ok: result.is_ok(),
            result_detail: Some(fmt_result_detail(&result)),
            elapsed: Duration::from_millis(10),
            speed: Duration::from_millis(11),
            blocked: false,
            lookup_source: LookupFrom::Server("default".to_string()),
        };

        let file = format!("./logs/test-{}-audit.log", Local::now().timestamp_millis());
        let file = Path::new(file.as_str());

        record_audit_to_file(
            &mut MappedFile::open(file, 102400, None, Default::default()),
            &[audit],
        )
        .unwrap();

        assert!(file.exists());

        let mut s = String::new();

        std::fs::File::open(file)
            .unwrap()
            .read_to_string(&mut s)
            .unwrap();

        assert_eq!(
            s,
            format!(
                "[{}] 127.0.0.1 query www.example.com, type: A, elapsed: 10ms, speed: 11ms, result 93.184.216.34 86400 A\n",
                now.format("%Y-%m-%d %H:%M:%S,%3f")
            )
        );

        std::fs::remove_file(file).unwrap();

        assert!(!file.exists());
    }

    #[test]
    fn test_record_audit_to_csv_file() {
        let query = Query::query(Name::from_str("www.example.com").unwrap(), RecordType::A);

        let result = Ok(DnsResponse::from_rdata(
            query.to_owned(),
            RData::A("93.184.216.34".parse().unwrap()),
        ));

        let audit1 = DnsAuditRecord {
            id: 11,
            date: "2022-11-11 20:18:11.099966887 +08:00".parse().unwrap(),
            client: "127.0.0.1".to_string(),
            query: query.clone(),
            result_ok: result.is_ok(),
            result_detail: Some(fmt_result_detail(&result)),
            elapsed: Duration::from_millis(10),
            speed: Duration::from_millis(11),
            blocked: false,
            lookup_source: LookupFrom::Server("default1".to_string()),
        };

        let audit2 = DnsAuditRecord {
            id: 12,
            date: "2022-11-11 20:18:11.099966887 +08:00".parse().unwrap(),
            client: "127.0.0.1".to_string(),
            query,
            result_ok: result.is_ok(),
            result_detail: Some(fmt_result_detail(&result)),
            elapsed: Duration::from_millis(10),
            speed: Duration::from_millis(11),
            blocked: false,
            lookup_source: LookupFrom::Server("default2".to_string()),
        };

        let file = format!("./logs/test-{}-audit.csv", Local::now().timestamp_millis());
        let file = Path::new(file.as_str());

        record_audit_to_file(
            &mut MappedFile::open(file, 102400, None, Default::default()),
            &[audit1],
        )
        .unwrap();

        assert!(file.exists());

        let mut s = String::new();

        std::fs::File::open(file)
            .unwrap()
            .read_to_string(&mut s)
            .unwrap();

        assert_eq!(
            s,
            "id,timestamp,client,name,type,elapsed,speed,state,result,lookup_source\n11,1668169091,127.0.0.1,www.example.com,A,10ms,11ms,success,93.184.216.34 86400 A,Server: default1\n"
        );

        record_audit_to_file(
            &mut MappedFile::open(file, 102400, None, Default::default()),
            &[audit2],
        )
        .unwrap();

        let mut s = String::new();

        std::fs::File::open(file)
            .unwrap()
            .read_to_string(&mut s)
            .unwrap();

        assert_eq!(
            s,
            "id,timestamp,client,name,type,elapsed,speed,state,result,lookup_source\n11,1668169091,127.0.0.1,www.example.com,A,10ms,11ms,success,93.184.216.34 86400 A,Server: default1\n12,1668169091,127.0.0.1,www.example.com,A,10ms,11ms,success,93.184.216.34 86400 A,Server: default2\n"
        );

        std::fs::remove_file(file).unwrap();

        assert!(!file.exists());
    }
}
