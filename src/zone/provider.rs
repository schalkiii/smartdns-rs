use crate::dns::{DnsContext, DnsError, DnsRequest, DnsResponse};

// async_trait 宏展开的生成函数在部分 clippy 版本会触发 clippy::double_must_use（宏产物误报，
// 非人为标注）；用 allow 兜底：既抑制旧版误报，又不会在新版 lint 未触发时因 expect 落空报错。
#[allow(
    clippy::double_must_use,
    reason = "async_trait 生成函数触发，宏产物误报"
)]
#[async_trait::async_trait]
pub trait ZoneProvider: Send + Sync {
    async fn lookup(
        &self,
        ctx: &DnsContext,
        req: &DnsRequest,
    ) -> Result<Option<DnsResponse>, DnsError>;
}
