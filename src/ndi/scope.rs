use std::ffi::CString;
use std::net::{IpAddr, SocketAddr};

/// Where sources are discovered: anywhere NDI's own discovery reaches, or
/// only at one IP address, which is then also queried directly (unicast).
///
/// A source on a host with several network interfaces (e.g. Wi-Fi and
/// Ethernet) is announced once per interface, and multicast may not cross
/// between them. Restricting discovery to one address picks the interface.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiscoveryScope {
    Network,
    Address(IpAddr),
}

impl DiscoveryScope {
    /// The `p_extra_ips` value for NDI's finder; `None` keeps NDI's default.
    pub(super) fn extra_ips(self) -> Option<CString> {
        match self {
            Self::Network => None,
            Self::Address(address) => {
                Some(CString::new(address.to_string()).expect("IP address contains no NUL"))
            }
        }
    }

    /// NDI URL addresses have the form `<ip>:<port>`.
    pub(super) fn includes(self, url_address: &str) -> bool {
        match self {
            Self::Network => true,
            Self::Address(address) => url_address
                .parse::<SocketAddr>()
                .is_ok_and(|socket| socket.ip() == address),
        }
    }

    /// Appended to messages that say where sources were looked for.
    pub fn location_suffix(self) -> String {
        match self {
            Self::Network => String::new(),
            Self::Address(address) => format!(" at {address}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const URL_AT_ADDRESS: &str = "192.168.1.20:5961";

    fn scope() -> DiscoveryScope {
        DiscoveryScope::Address("192.168.1.20".parse().unwrap())
    }

    #[test]
    fn network_includes_any_url() {
        assert!(DiscoveryScope::Network.includes("10.0.0.1:5961"));
    }

    #[test]
    fn address_includes_url_at_that_address() {
        assert!(scope().includes(URL_AT_ADDRESS));
    }

    #[test]
    fn address_excludes_url_at_other_address() {
        assert!(!scope().includes("192.168.1.200:5961"));
    }

    #[test]
    fn address_excludes_malformed_url() {
        assert!(!scope().includes("not-an-address"));
    }

    #[test]
    fn ipv6_address_includes_url_at_that_address() {
        let scope = DiscoveryScope::Address("fd00::20".parse().unwrap());
        assert!(scope.includes("[fd00::20]:5961"));
    }

    #[test]
    fn network_keeps_ndi_default_extra_ips() {
        assert_eq!(DiscoveryScope::Network.extra_ips(), None);
    }

    #[test]
    fn address_is_queried_directly() {
        let expected = CString::new("192.168.1.20").unwrap();
        assert_eq!(scope().extra_ips(), Some(expected));
    }
}
