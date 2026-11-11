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
    // /domain-sets 与 /adblock/stats 分开生成再 merge：
    // utoipa_axum 的 routes! 宏对多个 GET handler 会生成重叠路由
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
    /// 新自动更新间隔
    interval: Option<usize>,
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
        return Err(ApiError::Internal(anyhow::anyhow!(
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
        return Err(ApiError::Internal(anyhow::anyhow!(
            "domain-set name '{}' already exists",
            new_name
        )));
    }

    let ConfigLine::Config {
        config: ConfigItem::DomainSetProvider(old),
        ..
    } = &config[idx]
    else {
        unreachable!();
    };

    let new_provider = match old {
        DomainSetProvider::File(f) => DomainSetProvider::File(DomainSetFileProvider {
            name: new_name.clone(),
            file: f.file.clone(),
            content_type: f.content_type.clone(),
        }),
        DomainSetProvider::Http(h) => DomainSetProvider::Http(DomainSetHttpProvider {
            name: new_name.clone(),
            url: input
                .url
                .as_deref()
                .and_then(|u| Url::parse(u).ok())
                .unwrap_or_else(|| h.url.clone()),
            interval: input.interval.or(h.interval),
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
            return Err(ApiError::Internal(anyhow::anyhow!(
                "unknown content type: {other} (expect list/adblock)"
            )));
        }
    };

    if let Some(url) = &input.url {
        let url =
            Url::parse(url).map_err(|e| ApiError::Internal(anyhow::anyhow!("invalid url: {e}")))?;
        // 订阅下载只允许 http/https，杜绝 file:// 等 scheme 读本地任意文件
        if !matches!(url.scheme(), "http" | "https") {
            return Err(ApiError::Internal(anyhow::anyhow!(
                "unsupported url scheme: {} (expect http/https)",
                url.scheme()
            )));
        }
        Ok(DomainSetProvider::Http(DomainSetHttpProvider {
            name: input.name.clone(),
            url,
            interval: input.interval,
            content_type,
        }))
    } else if let Some(file) = &input.file {
        // 序列化按空格分词：含空白/控制字符的路径会破坏 round-trip
        if file.is_empty() || file.chars().any(|c| c.is_whitespace() || c.is_control()) {
            return Err(ApiError::Internal(anyhow::anyhow!(
                "invalid file path: {file:?} (whitespace/control chars not allowed)"
            )));
        }
        Ok(DomainSetProvider::File(DomainSetFileProvider {
            name: input.name.clone(),
            file: file.into(),
            content_type,
        }))
    } else {
        Err(ApiError::Internal(anyhow::anyhow!(
            "either url or file is required"
        )))
    }
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
        return Err(ApiError::Internal(anyhow::anyhow!(
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
