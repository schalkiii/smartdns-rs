use super::*;

impl NomParser for IpRule {
    fn parse(input: &str) -> IResult<&str, Self> {
        let (input, net) = NomParser::parse(input)?;

        let mut rule = IpRule {
            net,
            blacklist: false,
            whitelist: false,
            bogus_nxdomain: false,
            ignore: false,
        };

        let (input, flags) = many0(preceded(
            space1,
            alt((
                value(0u8, tag_no_case("-blacklist-ip")),
                value(1u8, tag_no_case("-whitelist-ip")),
                value(2u8, tag_no_case("-bogus-nxdomain")),
                value(3u8, tag_no_case("-ignore-ip")),
            )),
        ))
        .parse(input)?;

        for flag in flags {
            match flag {
                0 => rule.blacklist = true,
                1 => rule.whitelist = true,
                2 => rule.bogus_nxdomain = true,
                _ => rule.ignore = true,
            }
        }

        Ok((input, rule))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse() {
        let (rest, rule) = IpRule::parse("192.168.1.0/24 -blacklist-ip -ignore-ip").unwrap();
        assert_eq!(rest, "");
        assert_eq!(rule.net, "192.168.1.0/24".parse().unwrap());
        assert!(rule.blacklist);
        assert!(rule.ignore);
        assert!(!rule.whitelist);

        assert!(IpRule::parse("not-a-cidr -blacklist-ip").is_err());
    }
}
