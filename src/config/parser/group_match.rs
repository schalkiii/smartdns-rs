use super::*;

/// `group-match -g <group> -d <domain> -c <client-cidr>` 是语法糖：
/// `-d` 等价 `nameserver /domain/group`，`-c` 等价 `client-rules <cidr> -group <group>`。
impl NomParser for GroupMatch {
    fn parse(input: &str) -> IResult<&str, Self> {
        let mut group: Option<String> = None;
        let mut domains = Vec::new();
        let mut clients = Vec::new();

        let tokens = input.split_whitespace().collect::<Vec<_>>();
        let mut idx = 0;
        while idx < tokens.len() {
            let key = tokens[idx];
            let value = tokens.get(idx + 1).copied().ok_or_else(|| invalid(input))?;

            match key {
                "-g" | "--group" | "-group" => group = Some(value.to_string()),
                "-d" | "--domain" | "-domain" => {
                    let (_, domain) = NomParser::parse(value).map_err(|_| invalid(input))?;
                    domains.push(domain);
                }
                "-c" | "--client-ip" | "-client-ip" => {
                    let (_, client) = NomParser::parse(value).map_err(|_| invalid(input))?;
                    clients.push(client);
                }
                _ => return Err(invalid(input)),
            }

            idx += 2;
        }

        let group = group.ok_or_else(|| invalid(input))?;
        if domains.is_empty() && clients.is_empty() {
            return Err(invalid(input));
        }

        Ok((
            "",
            GroupMatch {
                group,
                domains,
                clients,
            },
        ))
    }
}

fn invalid(input: &str) -> nom::Err<nom::error::Error<&str>> {
    nom::Err::Error(nom::error::Error::new(input, nom::error::ErrorKind::Verify))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse() {
        let parsed = GroupMatch::parse("-g office -d example.com -c 192.168.1.0/24")
            .expect("group-match should parse");
        assert_eq!(parsed.0, "");
        assert_eq!(parsed.1.group, "office");
        assert_eq!(parsed.1.domains.len(), 1);
        assert_eq!(parsed.1.clients.len(), 1);

        // 缺少 -g 或没有任何匹配条件都应被拒绝
        assert!(GroupMatch::parse("-d example.com").is_err());
        assert!(GroupMatch::parse("-g office").is_err());
        assert!(GroupMatch::parse("-g office -x foo").is_err());
    }
}
