use crate::dns::{DnsContext, DnsError, DnsRequest, DnsResponse};

// async_trait 宏展开的生成函数会触发 clippy::double_must_use（宏产物误报，非人为标注）；
// 用 expect 显式登记，宏修复后此行会因 lint 未触发而编译报错，便于移除。
#[expect(
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
