use enum_dispatch::enum_dispatch;
use std::{collections::HashSet, path::PathBuf, str::FromStr};
use url::Url;

use anyhow::Result;

use super::WildcardName;

#[enum_dispatch(IDomainSetProvider)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DomainSetProvider {
    File(DomainSetFileProvider),
    Http(DomainSetHttpProvider),
}

/// domain-set 的解析结果。
///
/// 普通列表（`List`）只有一个集合；`Adblock` 格式额外携带例外集（`@@||domain^`），
/// 例外集在拦截判断之前查询——命中例外的域名放行，不应用任何 address 规则。
#[enum_dispatch]
pub trait IDomainSetProvider {
    fn name(&self) -> &str;

    fn get_domain_set(&self) -> Result<HashSet<WildcardName>>;

    /// adblock 格式的例外集；普通列表返回空集。
    fn get_allow_domain_set(&self) -> Result<HashSet<WildcardName>> {
        Ok(Default::default())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DomainSetFileProvider {
    pub name: String,
    pub file: PathBuf,
    pub content_type: DomainSetContentType,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DomainSetHttpProvider {
    pub name: String,
    pub url: Url,
    pub interval: Option<usize>,
    pub content_type: DomainSetContentType,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum DomainSetContentType {
    #[default]
    List,
    /// AdGuard/ABP 风格的 adblock 语法（`||domain^`、`@@||domain^`、hosts 行）
    Adblock,
}

impl IDomainSetProvider for DomainSetFileProvider {
    fn name(&self) -> &str {
        self.name.as_str()
    }

    fn get_domain_set(&self) -> Result<HashSet<WildcardName>> {
        let text = std::fs::read_to_string(&self.file)?;
        let mut domain_set = HashSet::new();
        match self.content_type {
            DomainSetContentType::List => read_to_domain_set(&text, &mut domain_set),
            DomainSetContentType::Adblock => read_to_domain_set_adblock(&text, &mut domain_set),
        }
        Ok(domain_set)
    }

    fn get_allow_domain_set(&self) -> Result<HashSet<WildcardName>> {
        match self.content_type {
            DomainSetContentType::List => Ok(Default::default()),
            DomainSetContentType::Adblock => {
                let text = std::fs::read_to_string(&self.file)?;
                let mut allow = HashSet::new();
                read_adblock_allow_set(&text, &mut allow);
                Ok(allow)
            }
        }
    }
}

impl IDomainSetProvider for DomainSetHttpProvider {
    fn name(&self) -> &str {
        self.name.as_str()
    }

    fn get_domain_set(&self) -> Result<HashSet<WildcardName>> {
        use crate::infra::http_client::{self, HttpResponse};

        let res = http_client::get(self.url.to_string())?;

        let text = res.text()?;
        let mut domain_set = HashSet::new();
        match self.content_type {
            DomainSetContentType::List => read_to_domain_set(&text, &mut domain_set),
            DomainSetContentType::Adblock => read_to_domain_set_adblock(&text, &mut domain_set),
        }
        Ok(domain_set)
    }

    fn get_allow_domain_set(&self) -> Result<HashSet<WildcardName>> {
        match self.content_type {
            DomainSetContentType::List => Ok(Default::default()),
            DomainSetContentType::Adblock => {
                use crate::infra::http_client::{self, HttpResponse};
                let res = http_client::get(self.url.to_string())?;
                let text = res.text()?;
                let mut allow = HashSet::new();
                read_adblock_allow_set(&text, &mut allow);
                Ok(allow)
            }
        }
    }
}

fn read_to_domain_set(s: &str, domain_set: &mut HashSet<WildcardName>) {
    for line in s.lines() {
        let line = line.trim_start();
        if line.starts_with('#') {
            continue;
        }
        let mut parts = line.split(' ');

        if let Some(n) = parts.next().and_then(|n| WildcardName::from_str(n).ok()) {
            domain_set.insert(n);
        }
    }
}

/// 解析 adblock 格式的拦截集：`||domain^` / 裸域名 / hosts 行（`0.0.0.0 domain`）。
/// `@@` 例外行、`##` 元素隐藏、`$` 修饰符、`/regex/`、`|http://` 等浏览器层语法跳过。
fn read_to_domain_set_adblock(s: &str, domain_set: &mut HashSet<WildcardName>) {
    for line in s.lines() {
        for domain in parse_adblock_line(line, false) {
            domain_set.insert(domain);
        }
    }
}

/// 解析 adblock 格式的例外集（`@@||domain^`）。拦截行返回空。
fn read_adblock_allow_set(s: &str, allow_set: &mut HashSet<WildcardName>) {
    for line in s.lines() {
        for domain in parse_adblock_line(line, true) {
            allow_set.insert(domain);
        }
    }
}

/// 解析单行 adblock 规则，产出 0..N 个域名。
///
/// `allow = true` 时只认例外规则（`@@` 前缀），`allow = false` 时只认拦截规则。
/// 拦截规则映射：`||example.com^` → `+.example.com`（子域通配，含裸域）。
/// hosts 行（`0.0.0.0 a.com b.com`）产出全部域名——多域名漏一个就漏拦截。
fn parse_adblock_line(line: &str, allow: bool) -> Vec<WildcardName> {
    let line = line.trim();
    // 注释（`!` 为 ABP 注释、`#` 为 hosts/部分列表注释）与空行
    if line.is_empty() || line.starts_with('!') || line.starts_with('#') {
        return vec![];
    }

    let (is_allow_rule, rest) = if let Some(rest) = line.strip_prefix("@@") {
        (true, rest)
    } else {
        (false, line)
    };
    if is_allow_rule != allow {
        return vec![];
    }

    // `||example.com^`：域锚定拦截（DNS 语义 = 拦截该域及全部子域）
    if let Some(rest) = rest.strip_prefix("||") {
        let domain = rest.split(['^', '|', '/', '$', ':']).next().unwrap_or("");
        return adblock_domain_to_wildcard(domain).into_iter().collect();
    }

    // `|http://...`：URL 前缀规则，DNS 层无 URL，跳过
    if rest.starts_with('|') || rest.contains("://") {
        return vec![];
    }

    // 元素隐藏/过滤规则（`##...`、`#$#...` 等）：浏览器层语法，DNS 层跳过整行
    if rest.contains("##") || rest.contains("#$#") || rest.contains("#%#") || rest.contains("#@#") {
        return vec![];
    }

    // hosts 格式行：`0.0.0.0 example.com`（IP 开头，域名在后续字段，全收）
    if rest
        .split_whitespace()
        .next()
        .is_some_and(|first| first.parse::<std::net::IpAddr>().is_ok())
    {
        return rest
            .split_whitespace()
            .skip(1)
            .filter_map(adblock_domain_to_wildcard)
            .collect();
    }

    // 裸域名或 `example.com^`：等价 `||example.com^`（AGH 对 DNS 拦截同样处理）
    let domain = rest.split(['^', '|', '/', '$', ':']).next().unwrap_or("");
    adblock_domain_to_wildcard(domain).into_iter().collect()
}

/// adblock 域名 → `Default(domain)`（匹配裸域 + 全部子域，与 `||domain^` 语义一致）。
/// 无效（空、含通配符中缀、正则标记）返回 None。
fn adblock_domain_to_wildcard(domain: &str) -> Option<WildcardName> {
    let domain = domain.trim();
    if domain.is_empty() || domain == "*" {
        return None;
    }
    // 跳过正则规则（`/regex/` 整行已被上游分割剔除，此处防御含 `*` 中缀的项）
    if domain.contains('*') {
        return None;
    }
    WildcardName::from_str(&domain.to_lowercase()).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_adblock_block_rules() {
        let text = "\
! Title: Test List\n\
||example.com^\n\
@@||allowed.com^\n\
||ads.tracker.cn^\n\
0.0.0.0 hosts-blocked.com\n\
127.0.0.1 hosts-blocked2.com\n\
baredomain.com^\n\
##.ad-banner\n\
example.com##div.ad\n\
/regexPattern/\n\
|http://urlrule.com/\n\
$third-party\n\
||modifier.com^$third-party\n\
";
        let mut block = HashSet::new();
        read_to_domain_set_adblock(text, &mut block);
        assert_eq!(
            block,
            [
                "example.com".parse::<WildcardName>().unwrap(),
                "ads.tracker.cn".parse().unwrap(),
                "hosts-blocked.com".parse().unwrap(),
                "hosts-blocked2.com".parse().unwrap(),
                "baredomain.com".parse().unwrap(),
                "modifier.com".parse().unwrap(),
            ]
            .into()
        );
    }

    #[test]
    fn test_parse_adblock_allow_rules() {
        let text = "\
@@||allowed.com^\n\
@@||sub.allowed.org^\n\
||blocked.com^\n\
@@baredomain-allow.com^\n\
";
        let mut allow = HashSet::new();
        read_adblock_allow_set(text, &mut allow);
        assert_eq!(
            allow,
            [
                "allowed.com".parse::<WildcardName>().unwrap(),
                "sub.allowed.org".parse().unwrap(),
                "baredomain-allow.com".parse().unwrap(),
            ]
            .into()
        );
    }

    #[test]
    fn test_adblock_hosts_line_multiple_domains() {
        // hosts 行多域名：全部收录（比 AGH 更宽容，避免漏拦截）
        let text = "0.0.0.0 a.com b.com c.com\n";
        let mut block = HashSet::new();
        read_to_domain_set_adblock(text, &mut block);
        assert!(block.contains(&"a.com".parse::<WildcardName>().unwrap()));
        assert!(block.contains(&"b.com".parse::<WildcardName>().unwrap()));
        assert!(block.contains(&"c.com".parse::<WildcardName>().unwrap()));
    }
}
