/// 下载体积上限：订阅/列表文件可能来自不可信 URL，防止巨文件 OOM。
const MAX_BODY_BYTES: u64 = 32 * 1024 * 1024;
/// 全局下载超时：慢 URL 不得无限阻塞调用方（含配置热更新路径）。
const DOWNLOAD_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

pub trait HttpResponse {
    fn text(self) -> anyhow::Result<String>;
}

#[cfg(feature = "ureq")]
pub fn get<T>(uri: T) -> Result<ureq::http::Response<ureq::Body>, ureq::Error>
where
    http::Uri: TryFrom<T>,
    <http::Uri as TryFrom<T>>::Error: Into<http::Error>,
{
    static AGENT: std::sync::LazyLock<ureq::Agent> = std::sync::LazyLock::new(|| {
        ureq::Agent::config_builder()
            .timeout_global(Some(DOWNLOAD_TIMEOUT))
            .build()
            .into()
    });
    let res = AGENT.get(uri).call()?;
    Ok(res)
}

#[cfg(feature = "ureq")]
impl HttpResponse for ureq::http::Response<ureq::Body> {
    fn text(self) -> anyhow::Result<String> {
        use std::io::Read;
        let mut buf = String::new();
        self.into_body()
            .into_reader()
            .take(MAX_BODY_BYTES)
            .read_to_string(&mut buf)?;
        Ok(buf)
    }
}

#[cfg(feature = "reqwest")]
use reqwest::blocking as http_client;

#[cfg(feature = "reqwest")]
pub fn get<T>(uri: T) -> Result<impl HttpResponse, reqwest::Error>
where
    T: reqwest::IntoUrl,
{
    let client = http_client::Client::builder()
        .timeout(DOWNLOAD_TIMEOUT)
        .build()?;
    let res = client.get(uri).send()?;
    Ok(res)
}

#[cfg(feature = "reqwest")]
impl HttpResponse for reqwest::blocking::Response {
    fn text(self) -> anyhow::Result<String> {
        use std::io::Read;
        let mut buf = String::new();
        self.take(MAX_BODY_BYTES).read_to_string(&mut buf)?;
        Ok(buf)
    }
}
