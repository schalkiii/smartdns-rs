use std::collections::VecDeque;
use std::io;
use std::io::Write;
use std::path::PathBuf;
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
}

impl From<&DnsAuditRecord> for QueryLogEntry {
    fn from(a: &DnsAuditRecord) -> Self {
        QueryLogEntry {
            time: a.date,
            client: a.client.clone(),
            domain: a.query.name().to_utf8(),
            query_type: a.query.query_type().to_string(),
            elapsed_ms: a.elapsed.as_secs_f64() * 1000.0,
            result: if a.result.is_ok() {
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
        }
    }
}

pub struct DnsAuditMiddleware {
    audit_sender: Sender<DnsAuditRecord>,
    query_log: Arc<Mutex<VecDeque<QueryLogEntry>>>,
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

        let duration = start.elapsed();

        let audit = DnsAuditRecord {
            id: req.id(),
            date: now,
            client: req.src().to_string(),
            query: req.query().original().to_owned(),
            result: res.clone(),
            elapsed: duration,
            speed: ctx.fastest_speed,
            lookup_source: ctx.source.clone(),
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

        // 审计发送不得阻塞请求返回路径：有界通道(容量100)满时直接丢弃（审计允许丢失），
        // 否则磁盘 IO 缓慢时所有 DNS 响应延迟都会被审计写盘速度拖住。
        if self.audit_sender.try_send(audit).is_err() {
            warn!("audit queue full, dropping audit record");
        }

        res
    }
}

impl DnsAuditMiddleware {
    /// 构建审计中间件。
    ///
    /// `query_log` 为 WebUI 查询日志环形缓冲（始终写入，无论是否启用磁盘审计）；
    /// `audit_file` 为可选的磁盘审计文件路径，仅在启用审计文件时传入，否则仅写入内存查询日志。
    pub fn new(
        query_log: Arc<Mutex<VecDeque<QueryLogEntry>>>,
        audit_file: Option<PathBuf>,
        audit_size: u64,
        audit_num: usize,
        mode: Option<u32>,
    ) -> Self {
        let (audit_tx, mut audit_rx) = mpsc::channel::<DnsAuditRecord>(100);

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
                        buf.clear();
                    }
                }
            });
        } else {
            // 无磁盘审计：丢弃文件写入通道，避免通道无限堆积。
            drop(audit_rx);
        }

        Self {
            audit_sender: audit_tx,
            query_log,
        }
    }
}

#[derive(Debug, Clone)]
pub struct DnsAuditRecord {
    id: u16,
    client: String,
    query: Query,
    result: Result<DnsResponse, DnsError>,
    speed: Duration,
    elapsed: Duration,
    date: DateTime<Local>,
    lookup_source: LookupFrom,
}

impl DnsAuditRecord {
    fn fmt_result(&self) -> String {
        if let Ok(lookup) = self.result.as_ref() {
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
                if audit.result.is_ok() {
                    "success"
                } else {
                    "failed"
                },
                audit.fmt_result().as_str(),
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
            result,
            elapsed: Duration::from_millis(10),
            speed: Duration::from_millis(11),
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
            result,
            elapsed: Duration::from_millis(10),
            speed: Duration::from_millis(11),
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
            result,
            elapsed: Duration::from_millis(10),
            speed: Duration::from_millis(11),
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
            result: result.clone(),
            elapsed: Duration::from_millis(10),
            speed: Duration::from_millis(11),
            lookup_source: LookupFrom::Server("default1".to_string()),
        };

        let audit2 = DnsAuditRecord {
            id: 12,
            date: "2022-11-11 20:18:11.099966887 +08:00".parse().unwrap(),
            client: "127.0.0.1".to_string(),
            query,
            result,
            elapsed: Duration::from_millis(10),
            speed: Duration::from_millis(11),
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
