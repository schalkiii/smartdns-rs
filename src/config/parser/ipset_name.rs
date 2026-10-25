use super::*;

impl NomParser for IpSetName {
    fn parse(input: &str) -> IResult<&str, Self> {
        use nom::bytes::complete::take_while1;

        let (rest, name) = take_while1(|c: char| !c.is_whitespace()).parse(input)?;
        Ok((rest, IpSetName(name.to_string())))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse() {
        let (rest, name) = IpSetName::parse("smartdns_v4").unwrap();
        assert_eq!(rest, "");
        assert_eq!(name.0, "smartdns_v4");
    }
}
