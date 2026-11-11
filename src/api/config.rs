use super::openapi::{
    IntoRouter, ToSchema,
    http::{get, post, put},
    routes,
};
use super::{ApiError, ServeState, StatefulRouter};
use crate::config::parser::parse_config;
use crate::libdns::proto::rr::Name;
use axum::Json;
use axum::extract::{Query, State};
use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

pub fn routes() -> StatefulRouter {
    // 本 fork 的 routes! 会把联合 MethodRouter 挂到调用内每个路径上，
    // 且同方法多 handler 直接 panic：必须按路径拆分调用（同 address.rs 单路径多方法写法）
    let r1 = routes![reload].into_router();
    let r2 = routes![config].into_router();
    let r3 = routes![get_options, put_options].into_router();
    let r4 = routes![get_lines, post_lines].into_router();
    r1.merge(r2).merge(r3).merge(r4)
}

#[post("/config/reload", tag = "Config")]
async fn reload(State(state): State<Arc<ServeState>>) -> Result<(), ApiError> {
    state.app.reload().await?;
    Ok(())
}

#[get("/config", tag = "Config", operation_id = "config")]
async fn config(State(state): State<Arc<ServeState>>) -> Json<ServerConfig> {
    let cfg = state.app.cfg().await;
    let conf_dir = cfg
        .conf_dir()
        .map(|p| std::fs::canonicalize(p).unwrap_or(p.to_path_buf()))
        .map(|p| p.to_string_lossy().into_owned());

    Json(ServerConfig {
        server_name: cfg.server_name(),
        conf_dir,
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
struct ServerConfig {
    #[schema(value_type = String)]
    server_name: Name,
    conf_dir: Option<String>,
}

// ---------------------------------------------------------------------------
// 主配置文件编辑 API
//
// 写回采用逐行文本手术而非 ConfigFile 全量序列化：绝大多数 ConfigItem 变体
// 未实现 Display（输出占位注释），全量往返会把用户配置抹掉。
// 标量选项（单值键）走 /config/options；多实例结构键（server*/bind*/address 等）
// 走 /config/lines 或各自的结构化 API（addresses/domain-sets）。
// ---------------------------------------------------------------------------

/// 单值标量键白名单（规范拼写）。多实例键不在此列。
const SCALAR_KEYS: &[&str] = &[
    "server-name",
    "num-workers",
    "mdns-lookup",
    "resolv-hostname",
    "hosts-file",
    "expand-ptr-from-address",
    "local-ptr-enable",
    "max-query-limit",
    "socket-buff-size",
    "user",
    "domain",
    "bind-cert-file",
    "bind-cert-key-file",
    "bind-cert-key-pass",
    "tcp-idle-time",
    "cache-file",
    "cache-persist",
    "cache-size",
    "cache-mem-size",
    "cache-checkpoint-time",
    "bind-cert-generate",
    "bind-cert-root-key-file",
    "bind-cert-san",
    "bind-cert-validity-days",
    "data-dir",
    "debug-save-fail-packet",
    "debug-save-fail-packet-dir",
    "odhcpd-lease-file",
    "local-domain",
    "speed-check-mode",
    "force-AAAA-SOA",
    "force-HTTPS-SOA",
    "force-no-CNAME",
    "acl-enable",
    "dualstack-ip-selection",
    "dualstack-ip-selection-threshold",
    "dualstack-ip-allow-force-AAAA",
    "dns64",
    "edns-client-subnet",
    "rr-ttl",
    "rr-ttl-min",
    "rr-ttl-max",
    "rr-ttl-reply-max",
    "local-ttl",
    "max-reply-ip-num",
    "response-mode",
    "log-console",
    "log-file-mode",
    "log-file",
    "log-filter",
    "log-level",
    "log-num",
    "log-size",
    "log-color",
    "log-syslog",
    "audit-enable",
    "audit-file",
    "audit-file-mode",
    "audit-num",
    "audit-size",
    "audit-SOA",
    "audit-console",
    "audit-syslog",
    "dnsmasq-lease-file",
    "ca-file",
    "ca-path",
    "ipset-timeout",
    "ipset-no-speed",
    "nftset-timeout",
    "nftset-debug",
    "nftset-no-speed",
    "resolv-file",
    "prefetch-domain",
    "serve-expired",
    "serve-expired-ttl",
    "serve-expired-reply-ttl",
    "serve-expired-prefetch-time",
];

/// 解析器接受的别名 → 规范键（`tag_no_case` 匹配，大小写不敏感）。
const SCALAR_ALIASES: &[(&str, &str)] = &[
    ("response", "response-mode"),
    ("resolv-hostanme", "resolv-hostname"),
];

/// 多实例结构键家族：/config/lines 按家族列出/增删改整行。
const LINE_FAMILIES: &[(&str, &[&str])] = &[
    (
        "server",
        &[
            "server",
            "server-udp",
            "server-tcp",
            "server-tls",
            "server-https",
            "server-quic",
            "server-h3",
            "server-http3",
        ],
    ),
    (
        "bind",
        &[
            "bind",
            "bind-udp",
            "bind-tcp",
            "bind-tls",
            "bind-https",
            "bind-http",
            "bind-quic",
            "bind-h3",
        ],
    ),
    ("forward", &["nameserver"]),
    ("cname", &["cname"]),
    ("txt-record", &["txt-record"]),
    ("srv-record", &["srv-record"]),
    ("https-record", &["https-record"]),
    ("domain-rule", &["domain-rule", "domain-rules"]),
    ("client-rule", &["client-rule", "client-rules"]),
    ("ip-rules", &["ip-rules"]),
    ("group-match", &["group-match"]),
    ("group", &["group-begin", "group-end"]),
    ("blacklist-ip", &["blacklist-ip"]),
    ("whitelist-ip", &["whitelist-ip"]),
    ("bogus-nxdomain", &["bogus-nxdomain"]),
    ("ignore-ip", &["ignore-ip"]),
    ("ip-set", &["ip-set"]),
    ("ip-alias", &["ip-alias"]),
    ("ipset", &["ipset"]),
    ("nftset", &["nftset"]),
    ("proxy-server", &["proxy-server"]),
    ("conf-file", &["conf-file"]),
    ("force-qtype-soa", &["force-qtype-soa"]),
];

fn canonical_scalar_key(key: &str) -> Option<&'static str> {
    SCALAR_KEYS
        .iter()
        .find(|k| k.eq_ignore_ascii_case(key))
        .copied()
        .or_else(|| {
            SCALAR_ALIASES
                .iter()
                .find(|(alias, _)| alias.eq_ignore_ascii_case(key))
                .map(|(_, canonical)| *canonical)
        })
}

fn family_keys(family: &str) -> Option<&'static [&'static str]> {
    LINE_FAMILIES
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case(family))
        .map(|(_, keys)| *keys)
}

fn key_in_family(key: &str, keys: &[&str]) -> bool {
    keys.iter().any(|k| k.eq_ignore_ascii_case(key))
}

/// 拆一行配置为 (前导空白, 键, 值, 尾注释)。注释行/空行返回 None。
/// 尾注释判定与解析器一致：`#` 前必须有空白。
fn split_conf_line(line: &str) -> Option<(&str, &str, &str, Option<&str>)> {
    let trimmed = line.trim_start();
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return None;
    }
    let lead = &line[..line.len() - trimmed.len()];
    let (key, rest) = match trimmed.split_once(char::is_whitespace) {
        Some((key, rest)) => (key, rest.trim_start()),
        None => (trimmed, ""),
    };
    let (value, comment) = match rest.find(" #").or_else(|| rest.find("\t#")) {
        Some(idx) => (rest[..idx].trim_end(), Some(rest[idx..].trim_start())),
        None => (rest.trim_end(), None),
    };
    Some((lead, key, value, comment))
}

/// 提取全局标量选项的当前值（同键多行时后者生效，与解析器赋值语义一致）。
/// group-begin/group-end 块内的行属于分组局部配置，跳过。
fn scalar_values(text: &str) -> BTreeMap<String, String> {
    let mut values = BTreeMap::new();
    let mut group_depth = 0usize;
    for line in text.lines() {
        let Some((_, key, value, _)) = split_conf_line(line) else {
            continue;
        };
        if key.eq_ignore_ascii_case("group-begin") {
            group_depth += 1;
            continue;
        }
        if key.eq_ignore_ascii_case("group-end") {
            group_depth = group_depth.saturating_sub(1);
            continue;
        }
        if group_depth > 0 {
            continue;
        }
        if let Some(canonical) = canonical_scalar_key(key) {
            values.insert(canonical.to_string(), value.to_string());
        }
    }
    values
}

/// 应用标量更新：Some(值) 替换所有同键行（保留各行尾注释），未出现则追加；
/// None 注释掉所有同键行（回落默认值，保留原文可恢复）。只动 group 块外的行。
fn apply_scalar_updates(text: &str, updates: &BTreeMap<String, Option<String>>) -> String {
    let mut found: HashSet<&str> = HashSet::new();
    let mut out: Vec<String> = Vec::new();
    let mut group_depth = 0usize;

    for line in text.lines() {
        let Some((lead, key, _, comment)) = split_conf_line(line) else {
            out.push(line.to_string());
            continue;
        };
        if key.eq_ignore_ascii_case("group-begin") {
            group_depth += 1;
            out.push(line.to_string());
            continue;
        }
        if key.eq_ignore_ascii_case("group-end") {
            group_depth = group_depth.saturating_sub(1);
            out.push(line.to_string());
            continue;
        }
        let canonical = if group_depth == 0 {
            canonical_scalar_key(key)
        } else {
            None
        };
        let Some(canonical) = canonical else {
            out.push(line.to_string());
            continue;
        };
        let Some(update) = updates.get(canonical) else {
            out.push(line.to_string());
            continue;
        };
        found.insert(canonical);
        match update {
            Some(value) => {
                let mut new_line = format!("{lead}{key} {value}");
                if let Some(comment) = comment {
                    new_line.push(' ');
                    new_line.push_str(comment);
                }
                out.push(new_line);
            }
            None => out.push(format!("{lead}# {}", line.trim_start())),
        }
    }

    for (key, update) in updates {
        if let Some(value) = update
            && !found.contains(key.as_str())
        {
            out.push(format!("{key} {value}"));
        }
    }

    let mut result = out.join("\n");
    if text.ends_with('\n') || !updates.is_empty() {
        result.push('\n');
    }
    result
}

/// 用真实解析器校验一行配置：必须完整消费且产出已知 ConfigItem。
fn validate_conf_line(key: &str, value: &str) -> Result<(), ApiError> {
    if value.chars().any(|c| c.is_control()) {
        return Err(ApiError::Internal(anyhow::anyhow!(
            "configuration value contains control characters: {key:?}"
        )));
    }
    let line = if value.is_empty() {
        key.to_string()
    } else {
        format!("{key} {value}")
    };
    match parse_config(&line) {
        Ok(("", Some(_))) => Ok(()),
        _ => Err(ApiError::Internal(anyhow::anyhow!(
            "invalid configuration line: {line:?}"
        ))),
    }
}

async fn read_conf_file(state: &ServeState) -> Result<(std::path::PathBuf, String), ApiError> {
    let cfg = state.app.cfg().await;
    let file = cfg
        .conf_file()
        .ok_or_else(|| ApiError::NotFound("no configuration file loaded".to_string()))?
        .to_path_buf();
    drop(cfg);
    let text = std::fs::read_to_string(&file)?;
    Ok((file, text))
}

#[derive(Debug, Serialize, ToSchema)]
struct ConfigOptions {
    conf_file: Option<String>,
    /// 键 → 当前配置值（仅主配置文件中显式写出的全局标量项）
    values: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize, ToSchema)]
struct UpdateConfigOptions {
    /// 键 → 新值；null 表示注释掉该项（回落默认值）
    options: BTreeMap<String, Option<String>>,
}

#[get("/config/options", tag = "Config")]
async fn get_options(
    State(state): State<Arc<ServeState>>,
) -> Result<Json<ConfigOptions>, ApiError> {
    let (file, text) = read_conf_file(&state).await?;
    Ok(Json(ConfigOptions {
        conf_file: Some(file.to_string_lossy().into_owned()),
        values: scalar_values(&text),
    }))
}

#[put("/config/options", tag = "Config")]
async fn put_options(
    State(state): State<Arc<ServeState>>,
    Json(input): Json<UpdateConfigOptions>,
) -> Result<Json<ConfigOptions>, ApiError> {
    let _guard = super::MANAGED_CONF_LOCK.lock().await;

    // 先归一化并整体校验，任何一项非法都不落盘
    let mut updates: BTreeMap<String, Option<String>> = BTreeMap::new();
    for (key, value) in &input.options {
        let canonical = canonical_scalar_key(key).ok_or_else(|| {
            ApiError::Internal(anyhow::anyhow!("unknown or non-scalar option: {key:?}"))
        })?;
        if let Some(value) = value {
            if value.is_empty() {
                return Err(ApiError::Internal(anyhow::anyhow!(
                    "empty value for {key:?}; use null to reset the option"
                )));
            }
            if value.contains('#') {
                return Err(ApiError::Internal(anyhow::anyhow!(
                    "scalar value must not contain '#': {key:?}"
                )));
            }
            validate_conf_line(canonical, value)?;
        }
        updates.insert(canonical.to_string(), value.clone());
    }

    let (file, text) = read_conf_file(&state).await?;
    let new_text = apply_scalar_updates(&text, &updates);
    super::write_managed_conf(&file, &new_text)?;
    state.app.reload().await?;

    Ok(Json(ConfigOptions {
        conf_file: Some(file.to_string_lossy().into_owned()),
        values: scalar_values(&new_text),
    }))
}

#[derive(Debug, Serialize, ToSchema)]
struct ConfigLineEntry {
    /// 行在文件中的 0 基索引（含注释/空行），用于 update/delete 定位
    index: usize,
    key: String,
    value: String,
    comment: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
struct ConfigLines {
    family: String,
    lines: Vec<ConfigLineEntry>,
}

#[derive(Debug, Deserialize, ToSchema)]
struct LinesQuery {
    family: String,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
enum LineAction {
    Add,
    Update,
    Delete,
}

#[derive(Debug, Deserialize, ToSchema)]
struct UpdateConfigLines {
    family: String,
    action: LineAction,
    key: String,
    #[serde(default)]
    value: String,
    /// update/delete 必填
    index: Option<usize>,
    /// update 时 Some 覆盖尾注释（空串=移除），None 保留原注释；add 时可选
    comment: Option<String>,
}

fn collect_family_lines(text: &str, keys: &[&str]) -> Vec<ConfigLineEntry> {
    text.lines()
        .enumerate()
        .filter_map(|(index, line)| {
            let (_, key, value, comment) = split_conf_line(line)?;
            if !key_in_family(key, keys) {
                return None;
            }
            Some(ConfigLineEntry {
                index,
                key: key.to_string(),
                value: value.to_string(),
                comment: comment.map(|c| c.trim_start_matches('#').trim().to_string()),
            })
        })
        .collect()
}

#[get("/config/lines", tag = "Config")]
async fn get_lines(
    State(state): State<Arc<ServeState>>,
    Query(query): Query<LinesQuery>,
) -> Result<Json<ConfigLines>, ApiError> {
    let keys = family_keys(&query.family).ok_or_else(|| {
        ApiError::NotFound(format!("unknown config line family: {:?}", query.family))
    })?;
    let (_, text) = read_conf_file(&state).await?;
    Ok(Json(ConfigLines {
        family: query.family,
        lines: collect_family_lines(&text, keys),
    }))
}

#[post("/config/lines", tag = "Config")]
async fn post_lines(
    State(state): State<Arc<ServeState>>,
    Json(input): Json<UpdateConfigLines>,
) -> Result<Json<ConfigLines>, ApiError> {
    let _guard = super::MANAGED_CONF_LOCK.lock().await;

    let keys = family_keys(&input.family).ok_or_else(|| {
        ApiError::NotFound(format!("unknown config line family: {:?}", input.family))
    })?;
    if !key_in_family(&input.key, keys) {
        return Err(ApiError::Internal(anyhow::anyhow!(
            "key {:?} does not belong to family {:?}",
            input.key,
            input.family
        )));
    }

    let (file, text) = read_conf_file(&state).await?;
    let mut lines: Vec<String> = text.lines().map(|l| l.to_string()).collect();

    match input.action {
        LineAction::Add => {
            validate_conf_line(&input.key, &input.value)?;
            let mut line = if input.value.is_empty() {
                input.key.clone()
            } else {
                format!("{} {}", input.key, input.value)
            };
            if let Some(comment) = &input.comment
                && !comment.is_empty()
            {
                line.push_str(&format!(" # {comment}"));
            }
            lines.push(line);
        }
        LineAction::Update | LineAction::Delete => {
            let index = input.index.ok_or_else(|| {
                ApiError::Internal(anyhow::anyhow!("index is required for update/delete"))
            })?;
            let current = lines
                .get(index)
                .ok_or_else(|| ApiError::NotFound(format!("line index {index} out of range")))?;
            let (_, current_key, _, current_comment) =
                split_conf_line(current).ok_or_else(|| {
                    ApiError::Internal(anyhow::anyhow!("line {index} is not a configuration line"))
                })?;
            // 索引可能因并发编辑过期：目标行必须仍属于本家族
            if !key_in_family(current_key, keys) {
                return Err(ApiError::Internal(anyhow::anyhow!(
                    "line {index} no longer belongs to family {:?}",
                    input.family
                )));
            }
            if matches!(input.action, LineAction::Delete) {
                lines.remove(index);
            } else {
                validate_conf_line(&input.key, &input.value)?;
                let lead = &current[..current.len() - current.trim_start().len()];
                let mut line = if input.value.is_empty() {
                    format!("{lead}{}", input.key)
                } else {
                    format!("{lead}{} {}", input.key, input.value)
                };
                let comment = input
                    .comment
                    .as_deref()
                    .or(current_comment.map(|c| c.trim_start_matches('#').trim()));
                if let Some(comment) = comment
                    && !comment.is_empty()
                {
                    line.push_str(&format!(" # {comment}"));
                }
                lines[index] = line;
            }
        }
    }

    let mut new_text = lines.join("\n");
    new_text.push('\n');
    super::write_managed_conf(&file, &new_text)?;
    state.app.reload().await?;

    Ok(Json(ConfigLines {
        family: input.family,
        lines: collect_family_lines(&new_text, keys),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_split_conf_line() {
        assert_eq!(
            split_conf_line("  cache-size 1024 # 条数上限"),
            Some(("  ", "cache-size", "1024", Some("# 条数上限")))
        );
        assert_eq!(
            split_conf_line("group-end"),
            Some(("", "group-end", "", None))
        );
        assert_eq!(split_conf_line("# comment"), None);
        assert_eq!(split_conf_line("   "), None);
        // '#' 前无空白不是注释（nftset 规格）
        assert_eq!(
            split_conf_line("nftset /a.com/#4:ip#t#s"),
            Some(("", "nftset", "/a.com/#4:ip#t#s", None))
        );
    }

    #[test]
    fn test_scalar_values_last_wins_and_group_skipped() {
        let text = "\
cache-size 100
cache-size 200
group-begin office
rr-ttl-min 60
group-end
rr-ttl-min 300
response fastest-response
";
        let values = scalar_values(text);
        assert_eq!(values.get("cache-size").unwrap(), "200");
        assert_eq!(values.get("rr-ttl-min").unwrap(), "300");
        // 别名归一化
        assert_eq!(values.get("response-mode").unwrap(), "fastest-response");
        assert!(!values.contains_key("response"));
    }

    #[test]
    fn test_apply_scalar_updates_replace_append_comment() {
        let text = "\
# 头部注释
cache-size 100 # 旧注释
log-level info
";
        let mut updates = BTreeMap::new();
        updates.insert("cache-size".to_string(), Some("200".to_string()));
        updates.insert("audit-enable".to_string(), Some("no".to_string()));
        updates.insert("log-level".to_string(), None);
        let out = apply_scalar_updates(text, &updates);
        assert!(out.contains("# 头部注释"));
        assert!(out.contains("cache-size 200 # 旧注释"));
        assert!(out.contains("audit-enable no"));
        assert!(out.contains("# log-level info"));
        assert!(!out.contains("\nlog-level info"));
    }

    #[test]
    fn test_apply_scalar_updates_skips_group_lines() {
        let text = "\
rr-ttl-min 300
group-begin office
rr-ttl-min 60
group-end
";
        let mut updates = BTreeMap::new();
        updates.insert("rr-ttl-min".to_string(), Some("120".to_string()));
        let out = apply_scalar_updates(text, &updates);
        assert!(out.contains("rr-ttl-min 120"));
        assert!(out.contains("group-begin office\nrr-ttl-min 60\ngroup-end"));
    }

    #[test]
    fn test_apply_scalar_updates_duplicate_lines_all_replaced() {
        let text = "cache-size 1\ncache-size 2\n";
        let mut updates = BTreeMap::new();
        updates.insert("cache-size".to_string(), Some("9".to_string()));
        let out = apply_scalar_updates(text, &updates);
        assert_eq!(out.matches("cache-size 9").count(), 2);
    }

    #[test]
    fn test_canonical_scalar_key_case_and_alias() {
        assert_eq!(canonical_scalar_key("Cache-Size"), Some("cache-size"));
        assert_eq!(canonical_scalar_key("response"), Some("response-mode"));
        assert_eq!(
            canonical_scalar_key("resolv-hostanme"),
            Some("resolv-hostname")
        );
        assert_eq!(canonical_scalar_key("server"), None);
        assert_eq!(canonical_scalar_key("no-such-key"), None);
    }

    #[test]
    fn test_validate_conf_line() {
        assert!(validate_conf_line("cache-size", "1024").is_ok());
        assert!(validate_conf_line("log-level", "info").is_ok());
        assert!(validate_conf_line("group-end", "").is_ok());
        // 非法取值 / 未知键 / 换行注入
        assert!(validate_conf_line("cache-size", "abc").is_err());
        assert!(validate_conf_line("no-such-key", "1").is_err());
        assert!(validate_conf_line("cache-size", "1\nserver 1.2.3.4").is_err());
    }

    #[test]
    fn test_collect_family_lines() {
        let text = "\
server 223.5.5.5
server-https https://doh.pub/dns-query -group cn # 加密上游
bind [::]:9053
# 注释
";
        let keys = family_keys("server").unwrap();
        let lines = collect_family_lines(text, keys);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].index, 0);
        assert_eq!(lines[0].key, "server");
        assert_eq!(lines[1].key, "server-https");
        assert_eq!(lines[1].value, "https://doh.pub/dns-query -group cn");
        assert_eq!(lines[1].comment.as_deref(), Some("加密上游"));

        let bind_lines = collect_family_lines(text, family_keys("bind").unwrap());
        assert_eq!(bind_lines.len(), 1);
        assert_eq!(bind_lines[0].index, 2);
    }

    #[test]
    fn test_family_keys_unknown() {
        assert!(family_keys("server").is_some());
        assert!(family_keys("no-such-family").is_none());
        assert!(key_in_family(
            "SERVER-HTTPS",
            family_keys("server").unwrap()
        ));
        assert!(!key_in_family("bind", family_keys("server").unwrap()));
    }
}
