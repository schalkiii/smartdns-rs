use std::sync::Arc;

use serde::{Deserialize, Serialize};
use url::Url;

use crate::config::{
    AddressRule, AddressRuleValue, Domain, DomainSetContentType, DomainSetFileProvider,
    DomainSetHttpProvider, DomainSetProvider, IDomainSetProvider,
    parser::{ConfigFile, ConfigItem, ConfigLine},
};

use super::openapi::{
    IntoRouter, ToSchema,
    http::{delete, get, post, put},
    routes,
};
use super::{ApiError, DataListPayload, ServeState, StatefulRouter};
use axum::{Json, extract::State, http::StatusCode};

pub fn routes() -> StatefulRouter {
    // 本 fork 的 routes! 会把联合 MethodRouter 挂到调用内每个路径上：
    // 不同路径必须分开调用再 merge（同路径不同方法可合并，见 AGENTS.md）
    let r1 = routes![list, create, update, delete].into_router();
    let r2 = routes![stats].into_router();
    r1.merge(r2)
}

/// WebUI 订阅条目：provider 配置 + 运行时加载统计。
#[derive(Debug, Serialize, ToSchema)]
struct DomainSetEntry {
    /// provider 名称（配置中的 -name）
    name: String,
    /// 来源类型：file / http
    source: String,
    /// 订阅地址（文件路径或 URL）
    location: String,
    /// 内容类型：list / adblock
    content_type: String,
    /// 自动更新间隔（秒，仅 http 订阅）
    interval: Option<usize>,
    /// 加载的拦截规则数（load 失败时为 0）
    rule_count: usize,
    /// 加载的例外规则数（@@ 规则，仅 adblock 格式）
    allow_count: usize,
    /// 是否启用（未生效的 provider 不出现在 domain_set_providers，恒 true）
    enabled: bool,
}

#[derive(Debug, Deserialize, ToSchema)]
struct CreateDomainSet {
    /// provider 名称
    name: String,
    /// 订阅 URL（与 file 二选一）
    url: Option<String>,
    /// 本地文件路径（与 url 二选一）
    file: Option<String>,
    /// 内容类型：list / adblock
    #[serde(default)]
    content_type: String,
    /// 自动更新间隔（秒，仅 url 订阅）
    interval: Option<usize>,
}

#[derive(Debug, Deserialize, ToSchema)]
struct UpdateDomainSet {
    /// 原 provider 名称
    name: String,
    /// 新名称（不变则传原值）
    new_name: Option<String>,
    /// 新订阅 URL
    url: Option<String>,
    /// 新自动更新间隔：None = 未传（保留原值）；Some(None) = 显式清除；
    /// Some(Some(n)) = 设为新值。serde 默认对 null 和缺省都给 None，
    /// 需 default + deserialize_with 组合才能区分「未传」与「传 null」
    #[serde(default, deserialize_with = "deserialize_double_option")]
    interval: Option<Option<usize>>,
}

/// `Option<Option<T>>` 反序列化：字段出现（含 null）→ Some(内层)，
/// 字段缺省（由 serde(default) 兜底）→ None。
fn deserialize_double_option<'de, T, D>(de: D) -> Result<Option<Option<T>>, D::Error>
where
    T: Deserialize<'de>,
    D: serde::Deserializer<'de>,
{
    Deserialize::deserialize(de).map(Some)
}

#[derive(Debug, Deserialize, ToSchema)]
struct DeleteDomainSet {
    name: String,
}

/// 订阅列表（含加载统计）。
#[get("/domain-sets", tag = "DomainSets", operation_id = "list_domain_sets")]
async fn list(State(state): State<Arc<ServeState>>) -> Json<DataListPayload<DomainSetEntry>> {
    let cfg = state.app.cfg().await;

    let mut data = Vec::new();
    for providers in cfg.domain_set_providers.values() {
        for p in providers {
            let stat = cfg.domain_set_stats().get(p.name());
            data.push(DomainSetEntry {
                name: p.name().to_string(),
                source: stat.map(|s| s.source.clone()).unwrap_or_default(),
                location: stat.map(|s| s.location.clone()).unwrap_or_default(),
                content_type: stat.map(|s| s.content_type.clone()).unwrap_or_default(),
                interval: match p {
                    DomainSetProvider::Http(h) => h.interval,
                    DomainSetProvider::File(_) => None,
                },
                rule_count: stat.map(|s| s.rule_count).unwrap_or_default(),
                allow_count: stat.map(|s| s.allow_count).unwrap_or_default(),
                enabled: true,
            });
        }
    }
    data.sort_by(|a, b| a.name.cmp(&b.name));

    Json(data.into())
}

/// 新增订阅（写入 managed/domain-set.conf 并热更新）。
#[post("/domain-sets", tag = "DomainSets", operation_id = "create_domain_set")]
async fn create(
    State(state): State<Arc<ServeState>>,
    Json(input): Json<CreateDomainSet>,
) -> Result<StatusCode, ApiError> {
    let _guard = super::MANAGED_CONF_LOCK.lock().await;
    let provider = build_provider(&input)?;

    let cfg = state.app.cfg().await;
    let Some(managed_dir) = cfg.managed_dir().map(|p| p.to_path_buf()) else {
        return Err(ApiError::NotFound("managed_dir not found".to_string()));
    };
    std::fs::create_dir_all(&managed_dir)?;
    let file = managed_dir.join("domain-set.conf");

    let text = if file.exists() {
        std::fs::read_to_string(&file)?
    } else {
        String::new()
    };
    let (_, mut config) = ConfigFile::parse(&text).map_err(|err| err.to_owned())?;

    // 名称唯一性检查
    if find_provider_idx(&config, &input.name).is_some() {
        return Err(ApiError::BadRequest(format!(
            "domain-set name '{}' already exists",
            input.name
        )));
    }

    config.push(ConfigLine::Config {
        config: ConfigItem::DomainSetProvider(provider),
        comment: None,
    });

    write_provider_only(&file, &mut config)?;

    // WebUI 订阅语义：添加即拦截。自动在 managed/address.conf 中补上
    // `address /domain-set:<name>/#`（SOA 拦截），无需用户手动关联。
    link_address_rule(&managed_dir, &input.name, true)?;
    drop(cfg);
    state.app.reload().await?;

    Ok(StatusCode::CREATED)
}

/// 更新订阅（改 URL / 名称 / interval），重写对应行并热更新。
#[put("/domain-sets", tag = "DomainSets", operation_id = "update_domain_set")]
async fn update(
    State(state): State<Arc<ServeState>>,
    Json(input): Json<UpdateDomainSet>,
) -> Result<StatusCode, ApiError> {
    let _guard = super::MANAGED_CONF_LOCK.lock().await;
    let cfg = state.app.cfg().await;
    let Some(managed_dir) = cfg.managed_dir().map(|p| p.to_path_buf()) else {
        return Err(ApiError::NotFound("managed_dir not found".to_string()));
    };
    let file = managed_dir.join("domain-set.conf");
    if !file.exists() {
        return Err(ApiError::NotFound(format!(
            "domain-set '{}' not found",
            input.name
        )));
    }

    let text = std::fs::read_to_string(&file)?;
    let (_, mut config) = ConfigFile::parse(&text).map_err(|err| err.to_owned())?;

    let Some(idx) = find_provider_idx(&config, &input.name) else {
        return Err(ApiError::NotFound(format!(
            "domain-set '{}' not found",
            input.name
        )));
    };

    let new_name = input.new_name.clone().unwrap_or_else(|| input.name.clone());
    validate_name(&new_name)?;

    // 改名时检查新名称冲突
    if new_name != input.name && find_provider_idx(&config, &new_name).is_some() {
        return Err(ApiError::BadRequest(format!(
            "domain-set name '{new_name}' already exists"
        )));
    }

    let ConfigLine::Config {
        config: ConfigItem::DomainSetProvider(old),
        ..
    } = &config[idx]
    else {
        unreachable!();
    };

    // 与 create 路径同强度校验：非法 URL 显式报 400，杜绝静默 fallback 旧值
    let new_url = match input.url.as_deref() {
        Some(u) => parse_subscription_url(u)?,
        None => match old {
            DomainSetProvider::Http(h) => h.url.clone(),
            // file provider 不存在 url 字段；update 语义只对 http 订阅开放
            DomainSetProvider::File(_) => {
                return Err(ApiError::BadRequest(format!(
                    "domain-set '{}' is a file provider, url update not supported",
                    input.name
                )));
            }
        },
    };

    let new_provider = match old {
        DomainSetProvider::File(f) => DomainSetProvider::File(DomainSetFileProvider {
            name: new_name.clone(),
            file: f.file.clone(),
            content_type: f.content_type.clone(),
        }),
        DomainSetProvider::Http(h) => DomainSetProvider::Http(DomainSetHttpProvider {
            name: new_name.clone(),
            url: new_url,
            interval: input.interval.unwrap_or(h.interval),
            content_type: h.content_type.clone(),
        }),
    };

    config[idx] = ConfigLine::Config {
        config: ConfigItem::DomainSetProvider(new_provider),
        comment: None,
    };

    write_provider_only(&file, &mut config)?;

    // 改名时联动迁移 address 拦截行，否则旧名失联、拦截静默失效
    if new_name != input.name {
        link_address_rule(&managed_dir, &input.name, false)?;
        link_address_rule(&managed_dir, &new_name, true)?;
    }
    drop(cfg);
    state.app.reload().await?;

    Ok(StatusCode::NO_CONTENT)
}

/// 删除订阅。
#[delete("/domain-sets", tag = "DomainSets", operation_id = "delete_domain_set")]
async fn delete(
    State(state): State<Arc<ServeState>>,
    Json(input): Json<DeleteDomainSet>,
) -> Result<StatusCode, ApiError> {
    let _guard = super::MANAGED_CONF_LOCK.lock().await;
    let cfg = state.app.cfg().await;
    let Some(managed_dir) = cfg.managed_dir().map(|p| p.to_path_buf()) else {
        return Err(ApiError::NotFound("managed_dir not found".to_string()));
    };
    let file = managed_dir.join("domain-set.conf");
    if !file.exists() {
        return Err(ApiError::NotFound(format!(
            "domain-set '{}' not found",
            input.name
        )));
    }

    let text = std::fs::read_to_string(&file)?;
    let (_, mut config) = ConfigFile::parse(&text).map_err(|err| err.to_owned())?;

    let Some(idx) = find_provider_idx(&config, &input.name) else {
        return Err(ApiError::NotFound(format!(
            "domain-set '{}' not found",
            input.name
        )));
    };

    config.remove(idx);
    write_provider_only(&file, &mut config)?;

    // 同步移除对应的 address 拦截行（若存在）
    link_address_rule(&managed_dir, &input.name, false)?;
    drop(cfg);
    state.app.reload().await?;

    Ok(StatusCode::NO_CONTENT)
}

/// 拦截统计：总拦截数 + 最近查询日志的 Top 拦截域名 / 客户端聚合。
#[derive(Debug, Serialize, ToSchema)]
struct AdblockStats {
    /// 自启动以来的拦截总数（address 规则 + adblock 订阅命中）
    blocked_total: u64,
    /// 最近查询日志（1000 条）中被拦截的域名 Top N
    top_blocked_domains: Vec<NameCount>,
    /// 最近查询日志（1000 条）中触发拦截的客户端 Top N
    top_blocked_clients: Vec<NameCount>,
}

#[derive(Debug, Serialize, ToSchema)]
struct NameCount {
    name: String,
    count: u64,
}

#[get("/adblock/stats", tag = "DomainSets", operation_id = "adblock_stats")]
async fn stats(State(state): State<Arc<ServeState>>) -> Json<AdblockStats> {
    let blocked_total = state.app.blocked_queries();

    // 从查询日志环形缓冲聚合（最近 N 条，与 /api/stats 同模式）
    let ring = state.app.query_log();
    let q = ring.lock().unwrap();

    let mut domains: std::collections::HashMap<String, u64> = Default::default();
    let mut clients: std::collections::HashMap<String, u64> = Default::default();
    for e in q.iter() {
        if e.blocked {
            *domains.entry(e.domain.clone()).or_default() += 1;
            *clients.entry(e.client.clone()).or_default() += 1;
        }
    }

    let mut top_blocked_domains: Vec<NameCount> = domains
        .into_iter()
        .map(|(name, count)| NameCount { name, count })
        .collect();
    top_blocked_domains.sort_by(|a, b| b.count.cmp(&a.count).then(a.name.cmp(&b.name)));
    top_blocked_domains.truncate(10);

    let mut top_blocked_clients: Vec<NameCount> = clients
        .into_iter()
        .map(|(name, count)| NameCount { name, count })
        .collect();
    top_blocked_clients.sort_by(|a, b| b.count.cmp(&a.count).then(a.name.cmp(&b.name)));
    top_blocked_clients.truncate(10);

    drop(q);

    Json(AdblockStats {
        blocked_total,
        top_blocked_domains,
        top_blocked_clients,
    })
}

// ---------- helpers ----------

fn build_provider(input: &CreateDomainSet) -> Result<DomainSetProvider, ApiError> {
    validate_name(&input.name)?;
    let content_type = match input.content_type.to_lowercase().as_str() {
        "" | "list" => DomainSetContentType::List,
        "adblock" => DomainSetContentType::Adblock,
        other => {
            return Err(ApiError::BadRequest(format!(
                "unknown content type: {other} (expect list/adblock)"
            )));
        }
    };

    if let Some(url) = &input.url {
        let url = parse_subscription_url(url)?;
        Ok(DomainSetProvider::Http(DomainSetHttpProvider {
            name: input.name.clone(),
            url,
            interval: input.interval,
            content_type,
        }))
    } else if let Some(file) = &input.file {
        // 序列化按空格分词：含空白/控制字符的路径会破坏 round-trip
        if file.is_empty() || file.chars().any(|c| c.is_whitespace() || c.is_control()) {
            return Err(ApiError::BadRequest(format!(
                "invalid file path: {file:?} (whitespace/control chars not allowed)"
            )));
        }
        Ok(DomainSetProvider::File(DomainSetFileProvider {
            name: input.name.clone(),
            file: file.into(),
            content_type,
        }))
    } else {
        Err(ApiError::BadRequest(
            "either url or file is required".to_string(),
        ))
    }
}

/// 订阅 URL 校验（create/update 共用）：Url::parse 必须成功，且 scheme
/// 限定 http/https——杜绝 file:// 等 scheme 读本地任意文件。
fn parse_subscription_url(input: &str) -> Result<Url, ApiError> {
    let url = Url::parse(input).map_err(|e| ApiError::BadRequest(format!("invalid url: {e}")))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(ApiError::BadRequest(format!(
            "unsupported url scheme: {} (expect http/https)",
            url.scheme()
        )));
    }
    Ok(url)
}

/// name 会原样序列化进配置行并被 `domain-set:<name>` 引用：
/// 限定字符集，避免空白/换行破坏解析或注入杂散配置行。
fn validate_name(name: &str) -> Result<(), ApiError> {
    if name.is_empty()
        || name.len() > 64
        || !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(ApiError::BadRequest(format!(
            "invalid domain-set name: {name:?} (expect 1-64 chars of [A-Za-z0-9_-])"
        )));
    }
    Ok(())
}

fn find_provider_idx(config: &ConfigFile<'_>, name: &str) -> Option<usize> {
    config.iter().position(|line| match line {
        ConfigLine::Config {
            config: ConfigItem::DomainSetProvider(p),
            ..
        } => p.name() == name,
        _ => false,
    })
}

/// 仅保留 domain-set 行与注释/空行后写回：domain-set.conf 由本 API 专托管，
/// 避免序列化其它未实现 Display 的 ConfigItem 变体（与 address.conf 同模式）。
fn write_provider_only(
    file: &std::path::Path,
    config: &mut ConfigFile<'_>,
) -> Result<(), ApiError> {
    config.retain(|line| {
        matches!(
            line,
            ConfigLine::Config {
                config: ConfigItem::DomainSetProvider(_),
                ..
            } | ConfigLine::Comment(_)
                | ConfigLine::EmptyLine
        )
    });
    super::write_managed_conf(file, &format!("{config}"))?;
    Ok(())
}

/// 维护 managed/address.conf 中的 `address /domain-set:<name>/#` 拦截行。
///
/// `link = true` 时追加（已存在则跳过）；`link = false` 时移除。
/// 只认领 SOA 族拦截行：用户手写的 `address /domain-set:<name>/<ip>` 绑定
/// 不属于订阅联动范围，增删订阅不得误删。
fn link_address_rule(
    managed_dir: &std::path::Path,
    name: &str,
    link: bool,
) -> Result<(), ApiError> {
    let file = managed_dir.join("address.conf");

    let target_domain = Domain::Set(name.to_string());
    let is_linked_rule = |rule: &AddressRule| {
        rule.domain == target_domain
            && matches!(
                rule.address,
                AddressRuleValue::SOA | AddressRuleValue::SOAv4 | AddressRuleValue::SOAv6
            )
    };

    // ConfigFile 借用解析文本：text 须与 config 同生命周期
    let text = if file.exists() {
        std::fs::read_to_string(&file)?
    } else {
        String::new()
    };
    let (_, mut config) = ConfigFile::parse(&text).map_err(|err| err.to_owned())?;

    // 仅处理 address 行（与 address API 同模式，其它行原样保留）
    let linked_idxs: Vec<usize> = config
        .iter()
        .enumerate()
        .filter_map(|(i, line)| match line {
            ConfigLine::Config {
                config: ConfigItem::Address(rule),
                ..
            } if is_linked_rule(rule) => Some(i),
            _ => None,
        })
        .collect();

    let changed = match link {
        true if linked_idxs.is_empty() => {
            config.push(ConfigLine::Config {
                config: ConfigItem::Address(AddressRule {
                    domain: target_domain,
                    address: AddressRuleValue::SOA,
                }),
                comment: None,
            });
            true
        }
        false if !linked_idxs.is_empty() => {
            for i in linked_idxs.iter().rev() {
                config.remove(*i);
            }
            true
        }
        _ => false, // 幂等
    };

    if changed {
        config.retain(|line| {
            matches!(
                line,
                ConfigLine::Config {
                    config: ConfigItem::Address(_),
                    ..
                } | ConfigLine::Comment(_)
                    | ConfigLine::EmptyLine
            )
        });
        super::write_managed_conf(&file, &format!("{config}"))?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 临时目录 + domain-set.conf + address.conf 组成的最小托管环境。
    /// 结构体持有目录路径，Drop 时递归清理，避免测试间残留。
    struct TempManagedDir {
        dir: std::path::PathBuf,
    }

    impl TempManagedDir {
        fn new(tag: &str) -> Self {
            let dir = std::env::temp_dir().join(format!(
                "smartdns-domain-set-test-{}-{tag}",
                std::process::id()
            ));
            std::fs::create_dir_all(&dir).unwrap();
            Self { dir }
        }

        fn write_domain_set_conf(&self, text: &str) {
            std::fs::write(self.dir.join("domain-set.conf"), text).unwrap();
        }

        fn read_domain_set_conf(&self) -> String {
            std::fs::read_to_string(self.dir.join("domain-set.conf")).unwrap()
        }
    }

    impl Drop for TempManagedDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    #[test]
    fn test_parse_subscription_url_rejects_invalid() {
        // 非法 URL
        assert!(parse_subscription_url("not a url").is_err());
        // scheme 限定 http/https
        assert!(parse_subscription_url("file:///etc/passwd").is_err());
        assert!(parse_subscription_url("ftp://example.com/list.txt").is_err());
        // 合法
        assert!(parse_subscription_url("https://example.com/list.txt").is_ok());
        assert!(parse_subscription_url("http://example.com/list.txt").is_ok());
    }

    #[test]
    fn test_build_provider_validates_name() {
        let input = CreateDomainSet {
            name: "bad name!".to_string(),
            url: Some("https://example.com/list.txt".to_string()),
            file: None,
            content_type: "list".to_string(),
            interval: None,
        };
        assert!(matches!(
            build_provider(&input),
            Err(ApiError::BadRequest(_))
        ));
    }

    #[test]
    fn test_build_provider_requires_url_or_file() {
        let input = CreateDomainSet {
            name: "ads".to_string(),
            url: None,
            file: None,
            content_type: "list".to_string(),
            interval: None,
        };
        assert!(matches!(
            build_provider(&input),
            Err(ApiError::BadRequest(_))
        ));
    }

    #[test]
    fn test_build_provider_unknown_content_type() {
        let input = CreateDomainSet {
            name: "ads".to_string(),
            url: Some("https://example.com/list.txt".to_string()),
            file: None,
            content_type: "hosts".to_string(),
            interval: None,
        };
        assert!(matches!(
            build_provider(&input),
            Err(ApiError::BadRequest(_))
        ));
    }

    #[test]
    fn test_build_provider_rejects_file_path_with_whitespace() {
        let input = CreateDomainSet {
            name: "ads".to_string(),
            url: None,
            file: Some("path with space/list.txt".to_string()),
            content_type: "list".to_string(),
            interval: None,
        };
        assert!(matches!(
            build_provider(&input),
            Err(ApiError::BadRequest(_))
        ));
    }

    /// update interval 双层 Option 语义：
    /// - None（字段未传）→ 保留原值
    /// - Some(None)（显式传 null）→ 清除
    /// - Some(Some(n)) → 设为新值
    /// 通过 serde_json 直接驱动反序列化，锁定 wire 协议语义。
    #[test]
    fn test_update_interval_double_option_semantics() {
        let json = r#"{"name": "ads", "interval": null}"#;
        let input: UpdateDomainSet = serde_json::from_str(json).unwrap();
        assert_eq!(input.interval, Some(None));

        let json = r#"{"name": "ads", "interval": 3600}"#;
        let input: UpdateDomainSet = serde_json::from_str(json).unwrap();
        assert_eq!(input.interval, Some(Some(3600)));

        let json = r#"{"name": "ads"}"#;
        let input: UpdateDomainSet = serde_json::from_str(json).unwrap();
        assert_eq!(input.interval, None);

        // 组合：interval 未传时 unwrap_or 保留原值
        let old_interval = Some(86400usize);
        assert_eq!(input.interval.unwrap_or(old_interval), Some(86400));
        // 显式清除
        let cleared: Option<Option<usize>> = Some(None);
        assert_eq!(cleared.unwrap_or(old_interval), None);
    }

    /// update 后写回的配置行 round-trip：interval 清除场景。
    /// 验证 Some(None) 语义最终落到配置文本（-interval 项消失）。
    #[test]
    fn test_update_clears_interval_in_config_round_trip() {
        let tmp = TempManagedDir::new("clear-interval");
        tmp.write_domain_set_conf(
            "domain-set -type adblock -name ads -url https://example.com/list.txt -interval 86400\n",
        );

        let text = tmp.read_domain_set_conf();
        let (_, mut config) = ConfigFile::parse(&text)
            .map_err(|err| err.to_owned())
            .unwrap();
        let idx = find_provider_idx(&config, "ads").unwrap();

        let input: UpdateDomainSet =
            serde_json::from_str(r#"{"name": "ads", "interval": null}"#).unwrap();

        let ConfigLine::Config {
            config: ConfigItem::DomainSetProvider(old),
            ..
        } = &config[idx]
        else {
            unreachable!();
        };
        let DomainSetProvider::Http(h) = old else {
            unreachable!();
        };

        let new_provider = DomainSetProvider::Http(DomainSetHttpProvider {
            name: "ads".to_string(),
            url: h.url.clone(),
            interval: input.interval.unwrap_or(h.interval),
            content_type: h.content_type.clone(),
        });
        config[idx] = ConfigLine::Config {
            config: ConfigItem::DomainSetProvider(new_provider),
            comment: None,
        };

        write_provider_only(&tmp.dir.join("domain-set.conf"), &mut config).unwrap();
        let out = tmp.read_domain_set_conf();
        assert!(
            out.contains("-url https://example.com/list.txt"),
            "url preserved: {out}"
        );
        assert!(
            !out.contains("-interval"),
            "interval must be cleared: {out}"
        );
    }

    /// update 后写回的配置行 round-trip：interval 改值场景（Some(Some(n))）。
    #[test]
    fn test_update_changes_interval_in_config_round_trip() {
        let tmp = TempManagedDir::new("change-interval");
        tmp.write_domain_set_conf(
            "domain-set -type adblock -name ads -url https://example.com/list.txt -interval 86400\n",
        );

        let text = tmp.read_domain_set_conf();
        let (_, mut config) = ConfigFile::parse(&text)
            .map_err(|err| err.to_owned())
            .unwrap();
        let idx = find_provider_idx(&config, "ads").unwrap();

        let input: UpdateDomainSet =
            serde_json::from_str(r#"{"name": "ads", "interval": 3600}"#).unwrap();

        let ConfigLine::Config {
            config: ConfigItem::DomainSetProvider(old),
            ..
        } = &config[idx]
        else {
            unreachable!();
        };
        let DomainSetProvider::Http(h) = old else {
            unreachable!();
        };

        let new_provider = DomainSetProvider::Http(DomainSetHttpProvider {
            name: "ads".to_string(),
            url: h.url.clone(),
            interval: input.interval.unwrap_or(h.interval),
            content_type: h.content_type.clone(),
        });
        config[idx] = ConfigLine::Config {
            config: ConfigItem::DomainSetProvider(new_provider),
            comment: None,
        };

        write_provider_only(&tmp.dir.join("domain-set.conf"), &mut config).unwrap();
        let out = tmp.read_domain_set_conf();
        assert!(out.contains("-interval 3600"), "interval updated: {out}");
    }

    #[test]
    fn test_find_provider_idx() {
        let text = "\
domain-set -type list -name a -url https://a.example.com/l.txt
# comment
domain-set -type adblock -name b -url https://b.example.com/l.txt -interval 60
";
        let (_, config) = ConfigFile::parse(text)
            .map_err(|err| err.to_owned())
            .unwrap();
        assert_eq!(find_provider_idx(&config, "a"), Some(0));
        assert_eq!(find_provider_idx(&config, "b"), Some(2));
        assert_eq!(find_provider_idx(&config, "c"), None);
    }

    #[test]
    fn test_link_address_rule_add_and_remove() {
        let tmp = TempManagedDir::new("link-address");

        // 添加拦截行
        link_address_rule(&tmp.dir, "ads", true).unwrap();
        let text = std::fs::read_to_string(tmp.dir.join("address.conf")).unwrap();
        assert!(text.contains("address /domain-set:ads/#"));

        // 幂等：重复添加不产生重复行
        link_address_rule(&tmp.dir, "ads", true).unwrap();
        let text = std::fs::read_to_string(tmp.dir.join("address.conf")).unwrap();
        assert_eq!(text.matches("domain-set:ads").count(), 1);

        // 用户手写的 IP 绑定行不被误删
        std::fs::write(
            tmp.dir.join("address.conf"),
            "address /domain-set:ads/#\naddress /domain-set:ads/192.168.1.1\n",
        )
        .unwrap();
        link_address_rule(&tmp.dir, "ads", false).unwrap();
        let text = std::fs::read_to_string(tmp.dir.join("address.conf")).unwrap();
        assert!(!text.contains("/#"));
        assert!(text.contains("address /domain-set:ads/192.168.1.1"));
    }
}
