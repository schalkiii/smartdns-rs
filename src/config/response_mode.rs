/// response mode
///
/// response-mode [first-ping|fastest-ip|fastest-response]
#[derive(Default, Debug, Copy, Clone, Hash, PartialEq, Eq)]
pub enum ResponseMode {
    #[default]
    FirstPing,
    FastestIp,
    FastestResponse,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_is_first_ping() {
        assert_eq!(ResponseMode::default(), ResponseMode::FirstPing);
    }

    #[test]
    fn test_equality() {
        assert_eq!(ResponseMode::FirstPing, ResponseMode::FirstPing);
        assert_ne!(ResponseMode::FirstPing, ResponseMode::FastestIp);
    }

    #[test]
    fn test_debug() {
        assert_eq!(format!("{:?}", ResponseMode::FirstPing), "FirstPing");
        assert_eq!(format!("{:?}", ResponseMode::FastestIp), "FastestIp");
        assert_eq!(
            format!("{:?}", ResponseMode::FastestResponse),
            "FastestResponse"
        );
    }

    #[test]
    fn test_clone() {
        let mode = ResponseMode::FastestIp;
        assert_eq!(mode, mode.clone());
    }
}
