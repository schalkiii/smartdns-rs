use std::net::{IpAddr, SocketAddr};

use crate::infra::ping::PingAddr;

#[derive(Copy, Clone, PartialEq, Eq, Hash)]
pub enum SpeedCheckMode {
    None,
    Ping,
    Tcp(u16),
    Http(u16),
    Https(u16),
}

impl SpeedCheckMode {
    pub fn is_none(&self) -> bool {
        matches!(self, SpeedCheckMode::None)
    }

    pub fn to_ping_addr(self, ip_addr: IpAddr) -> Option<PingAddr> {
        use SpeedCheckMode::*;
        Some(match self {
            None => return Default::default(),
            Ping => PingAddr::Icmp(ip_addr),
            Tcp(port) => PingAddr::Tcp(SocketAddr::new(ip_addr, port)),
            Http(port) => PingAddr::Http(SocketAddr::new(ip_addr, port)),
            Https(port) => PingAddr::Https(SocketAddr::new(ip_addr, port)),
        })
    }

    pub fn to_ping_addrs(self, ip_addrs: &[IpAddr]) -> Vec<PingAddr> {
        ip_addrs
            .iter()
            .flat_map(|ip| self.to_ping_addr(*ip))
            .collect()
    }
}

impl std::fmt::Debug for SpeedCheckMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use SpeedCheckMode::*;
        match self {
            None => write!(f, "None"),
            Ping => write!(f, "ICMP"),
            Tcp(port) => write!(f, "TCP:{port}"),
            Http(port) => {
                if *port == 80 {
                    write!(f, "HTTP")
                } else {
                    write!(f, "HTTP:{port}")
                }
            }
            Https(port) => {
                if *port == 443 {
                    write!(f, "HTTPS")
                } else {
                    write!(f, "HTTPS:{port}")
                }
            }
        }
    }
}

#[derive(Clone, PartialEq, Eq, Hash)]
pub struct SpeedCheckModeList(pub Vec<SpeedCheckMode>);

impl SpeedCheckModeList {
    pub fn push(&mut self, mode: SpeedCheckMode) -> Option<SpeedCheckMode> {
        if self.0.iter().all(|m| m != &mode) {
            self.0.push(mode);
            None
        } else {
            Some(mode)
        }
    }
}

impl From<Vec<SpeedCheckMode>> for SpeedCheckModeList {
    fn from(value: Vec<SpeedCheckMode>) -> Self {
        let mut lst = Self(Vec::with_capacity(value.len()));
        for mode in value {
            lst.push(mode);
        }
        lst
    }
}

impl std::fmt::Debug for SpeedCheckModeList {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (i, m) in self.0.iter().enumerate() {
            let last = i == self.len() - 1;
            write!(f, "{:?}{}", m, if !last { ", " } else { "" })?;
        }
        Ok(())
    }
}

impl std::ops::Deref for SpeedCheckModeList {
    type Target = Vec<SpeedCheckMode>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::ops::DerefMut for SpeedCheckModeList {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl std::default::Default for SpeedCheckModeList {
    fn default() -> Self {
        Self(vec![
            SpeedCheckMode::Ping,
            SpeedCheckMode::Http(80),
            SpeedCheckMode::Https(443),
        ])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    #[test]
    fn test_is_none() {
        assert!(SpeedCheckMode::None.is_none());
        assert!(!SpeedCheckMode::Ping.is_none());
        assert!(!SpeedCheckMode::Tcp(80).is_none());
        assert!(!SpeedCheckMode::Http(80).is_none());
        assert!(!SpeedCheckMode::Https(443).is_none());
    }

    #[test]
    fn test_to_ping_addr_none() {
        let ip = IpAddr::V4(Ipv4Addr::new(1, 1, 1, 1));
        assert!(SpeedCheckMode::None.to_ping_addr(ip).is_none());
    }

    #[test]
    fn test_to_ping_addr_ping() {
        let ip = IpAddr::V4(Ipv4Addr::new(1, 1, 1, 1));
        let result = SpeedCheckMode::Ping.to_ping_addr(ip);
        assert!(matches!(result, Some(PingAddr::Icmp(_))));
    }

    #[test]
    fn test_to_ping_addr_tcp() {
        let ip = IpAddr::V4(Ipv4Addr::new(1, 1, 1, 1));
        let result = SpeedCheckMode::Tcp(80).to_ping_addr(ip);
        assert!(matches!(result, Some(PingAddr::Tcp(_))));
    }

    #[test]
    fn test_to_ping_addr_http() {
        let ip = IpAddr::V4(Ipv4Addr::new(1, 1, 1, 1));
        let result = SpeedCheckMode::Http(80).to_ping_addr(ip);
        assert!(matches!(result, Some(PingAddr::Http(_))));
    }

    #[test]
    fn test_to_ping_addr_https() {
        let ip = IpAddr::V4(Ipv4Addr::new(1, 1, 1, 1));
        let result = SpeedCheckMode::Https(443).to_ping_addr(ip);
        assert!(matches!(result, Some(PingAddr::Https(_))));
    }

    #[test]
    fn test_to_ping_addrs() {
        let ips = vec![
            IpAddr::V4(Ipv4Addr::new(1, 1, 1, 1)),
            IpAddr::V4(Ipv4Addr::new(8, 8, 8, 8)),
        ];
        let result = SpeedCheckMode::Ping.to_ping_addrs(&ips);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_debug_format() {
        assert_eq!(format!("{:?}", SpeedCheckMode::None), "None");
        assert_eq!(format!("{:?}", SpeedCheckMode::Ping), "ICMP");
        assert_eq!(format!("{:?}", SpeedCheckMode::Tcp(8080)), "TCP:8080");
        assert_eq!(format!("{:?}", SpeedCheckMode::Http(80)), "HTTP");
        assert_eq!(format!("{:?}", SpeedCheckMode::Http(8080)), "HTTP:8080");
        assert_eq!(format!("{:?}", SpeedCheckMode::Https(443)), "HTTPS");
        assert_eq!(format!("{:?}", SpeedCheckMode::Https(8443)), "HTTPS:8443");
    }

    #[test]
    fn test_speed_check_mode_list_push_no_dup() {
        let mut list = SpeedCheckModeList::default();
        let result = list.push(SpeedCheckMode::Tcp(53));
        assert!(result.is_none());
        assert_eq!(list.len(), 4);
    }

    #[test]
    fn test_speed_check_mode_list_push_dup() {
        let mut list = SpeedCheckModeList::default();
        let result = list.push(SpeedCheckMode::Ping);
        assert!(result.is_some());
        assert_eq!(list.len(), 3);
    }

    #[test]
    fn test_speed_check_mode_list_default() {
        let list = SpeedCheckModeList::default();
        assert_eq!(list.len(), 3);
        assert!(list.contains(&SpeedCheckMode::Ping));
        assert!(list.contains(&SpeedCheckMode::Http(80)));
        assert!(list.contains(&SpeedCheckMode::Https(443)));
    }

    #[test]
    fn test_speed_check_mode_list_debug() {
        let list = SpeedCheckModeList::default();
        let debug_str = format!("{:?}", list);
        assert!(debug_str.contains("ICMP"));
        assert!(debug_str.contains("HTTP"));
        assert!(debug_str.contains("HTTPS"));
    }

    #[test]
    fn test_speed_check_mode_list_from_vec() {
        let list: SpeedCheckModeList = vec![
            SpeedCheckMode::Ping,
            SpeedCheckMode::Ping,
            SpeedCheckMode::Tcp(53),
        ]
        .into();
        assert_eq!(list.len(), 2);
    }
}
