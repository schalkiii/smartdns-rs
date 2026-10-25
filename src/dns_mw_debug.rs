use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::dns::*;
use crate::log;
use crate::middleware::*;

/// `debug-save-fail-packet yes`：把查询失败的请求报文写入磁盘，便于排障。
pub struct DnsFailPacketMiddleware;

#[async_trait::async_trait]
impl Middleware<DnsContext, DnsRequest, DnsResponse, DnsError> for DnsFailPacketMiddleware {
    async fn handle(
        &self,
        ctx: &mut DnsContext,
        req: &DnsRequest,
        next: Next<'_, DnsContext, DnsRequest, DnsResponse, DnsError>,
    ) -> Result<DnsResponse, DnsError> {
        let res = next.run(ctx, req).await;

        if res.is_err() && ctx.cfg().debug_save_fail_packet() {
            let dir = ctx.cfg().debug_save_fail_packet_dir();
            let name = req.query().name().to_utf8().replace('.', "_");
            if let Ok(buf) = req.message().to_vec() {
                // 写盘放阻塞线程，避免拖慢失败响应
                tokio::task::spawn_blocking(move || write_fail_packet(&dir, &name, &buf));
            }
        }

        res
    }
}

/// 写入 `<dir>/client-<name>-<unix>.pkt`。
fn write_fail_packet(dir: &Path, name: &str, packet: &[u8]) -> Option<PathBuf> {
    if let Err(err) = std::fs::create_dir_all(dir) {
        log::warn!(
            "save fail packet: create dir {} failed: {err}",
            dir.display()
        );
        return None;
    }

    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_default();
    let path = dir.join(format!("client-{name}-{stamp}.pkt"));

    match std::fs::write(&path, packet) {
        Ok(()) => Some(path),
        Err(err) => {
            log::warn!("save fail packet: write {} failed: {err}", path.display());
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_write_fail_packet() {
        let dir = std::env::temp_dir().join("smartdns-test-fail-packet");
        let _ = std::fs::remove_dir_all(&dir);

        let path = write_fail_packet(&dir, "example_com", b"packet-bytes").unwrap();

        assert!(path.exists());
        assert_eq!(std::fs::read(&path).unwrap(), b"packet-bytes");
        assert!(
            path.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("client-example_com-")
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
}
