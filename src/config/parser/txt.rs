use super::*;

impl NomParser for TxtRecord {
    fn parse(input: &str) -> IResult<&str, Self> {
        use nom::combinator::{rest, verify};

        map(
            verify(rest, |text: &str| !text.trim().is_empty()),
            |text: &str| TxtRecord(text.trim().to_string()),
        )
        .parse(input)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse() {
        assert_eq!(
            ConfigForDomain::<TxtRecord>::parse("/example.com/v=spf1 -all"),
            Ok((
                "",
                ConfigForDomain {
                    domain: Domain::Name("example.com".parse().unwrap()),
                    config: TxtRecord("v=spf1 -all".to_string()),
                }
            ))
        );

        // 与 C 版一致：没有 /domain/ 前缀时落到根域，整行作为文本
        let (rest, parsed) = ConfigForDomain::<TxtRecord>::parse("bare text").unwrap();
        assert_eq!(rest, "");
        assert_eq!(parsed.config, TxtRecord("bare text".to_string()));
        assert!(matches!(
            parsed.domain,
            Domain::Name(WildcardName::Default(_))
        ));

        assert!(ConfigForDomain::<TxtRecord>::parse("/example.com/   ").is_err());
    }
}
