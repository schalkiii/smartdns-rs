use super::*;

impl NomParser for LocalDomain {
    fn parse(input: &str) -> IResult<&str, Self> {
        alt((
            map(char('-'), |_| LocalDomain(None)),
            map(NomParser::parse, |name| LocalDomain(Some(name))),
        ))
        .parse(input)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse() {
        assert_eq!(
            LocalDomain::parse("lan"),
            Ok(("", LocalDomain(Some("lan".parse().unwrap()))))
        );
        assert_eq!(LocalDomain::parse("-"), Ok(("", LocalDomain(None))));
    }
}
