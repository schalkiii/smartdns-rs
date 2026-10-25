use super::*;

/// `bind-cert-san` 的取值：逗号分隔的域名/IP 列表。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SanList(pub Vec<String>);

impl NomParser for SanList {
    fn parse(input: &str) -> IResult<&str, Self> {
        use nom::bytes::complete::take_while1;

        let (rest, value) = take_while1(|c: char| !c.is_whitespace()).parse(input)?;
        Ok((
            rest,
            SanList(
                value
                    .split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(|s| s.trim_end_matches('.').to_string())
                    .collect(),
            ),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse() {
        let (rest, list) = SanList::parse("dns.example.com,1.2.3.4,::1").unwrap();
        assert_eq!(rest, "");
        assert_eq!(
            list.0,
            vec![
                "dns.example.com".to_string(),
                "1.2.3.4".to_string(),
                "::1".to_string()
            ]
        );
    }
}
