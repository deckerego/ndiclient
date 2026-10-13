use std::fmt;
use std::net::{IpAddr, SocketAddr};

/// Which NDI source to connect to: any source on a host, or one by name.
pub struct SourceQuery {
    host: IpAddr,
    source_name: Option<String>,
}

impl SourceQuery {
    pub fn any_on(host: IpAddr) -> Self {
        Self {
            host,
            source_name: None,
        }
    }

    /// `source_name` may be the short name (`OBS PGM`) or the full NDI name
    /// (`MACHINE (OBS PGM)`).
    pub fn named(host: IpAddr, source_name: String) -> Self {
        Self {
            host,
            source_name: Some(source_name),
        }
    }

    pub fn host(&self) -> IpAddr {
        self.host
    }

    pub fn matches(&self, full_name: &str, url_address: &str) -> bool {
        self.is_on_host(url_address) && self.matches_name(full_name)
    }

    pub fn is_on_host(&self, url_address: &str) -> bool {
        is_url_hosted_at(url_address, self.host)
    }

    fn matches_name(&self, full_name: &str) -> bool {
        self.source_name
            .as_deref()
            .is_none_or(|wanted| full_name == wanted || short_name_of(full_name) == Some(wanted))
    }
}

impl fmt::Display for SourceQuery {
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        match &self.source_name {
            Some(name) => write!(formatter, "NDI source \"{name}\" at {}", self.host),
            None => write!(formatter, "an NDI source at {}", self.host),
        }
    }
}

/// NDI URL addresses have the form `<ip>:<port>`.
fn is_url_hosted_at(url_address: &str, host: IpAddr) -> bool {
    url_address
        .parse::<SocketAddr>()
        .is_ok_and(|address| address.ip() == host)
}

/// NDI full names have the form `MACHINE (Source Name)`.
fn short_name_of(full_name: &str) -> Option<&str> {
    let (_machine, rest) = full_name.split_once(" (")?;
    rest.strip_suffix(')')
}

#[cfg(test)]
mod tests {
    use super::*;

    const URL_ON_HOST: &str = "192.168.1.20:5961";
    const FULL_NAME: &str = "STUDIO.LOCAL (OBS PGM)";

    fn host() -> IpAddr {
        "192.168.1.20".parse().unwrap()
    }

    #[test]
    fn url_on_same_host_matches() {
        assert!(is_url_hosted_at(URL_ON_HOST, host()));
    }

    #[test]
    fn url_on_other_host_does_not_match() {
        assert!(!is_url_hosted_at("192.168.1.200:5961", host()));
    }

    #[test]
    fn malformed_url_does_not_match() {
        assert!(!is_url_hosted_at("not-an-address", host()));
    }

    #[test]
    fn unnamed_query_matches_any_source_on_host() {
        assert!(SourceQuery::any_on(host()).matches(FULL_NAME, URL_ON_HOST));
    }

    #[test]
    fn named_query_matches_short_name() {
        let query = SourceQuery::named(host(), "OBS PGM".to_string());
        assert!(query.matches(FULL_NAME, URL_ON_HOST));
    }

    #[test]
    fn named_query_matches_full_name() {
        let query = SourceQuery::named(host(), FULL_NAME.to_string());
        assert!(query.matches(FULL_NAME, URL_ON_HOST));
    }

    #[test]
    fn named_query_rejects_other_name() {
        let query = SourceQuery::named(host(), "Test Patterns".to_string());
        assert!(!query.matches(FULL_NAME, URL_ON_HOST));
    }

    #[test]
    fn named_query_rejects_partial_name() {
        let query = SourceQuery::named(host(), "OBS".to_string());
        assert!(!query.matches(FULL_NAME, URL_ON_HOST));
    }

    #[test]
    fn named_query_rejects_matching_name_on_other_host() {
        let query = SourceQuery::named(host(), "OBS PGM".to_string());
        assert!(!query.matches(FULL_NAME, "192.168.1.200:5961"));
    }

    #[test]
    fn short_name_handles_parentheses_inside_name() {
        assert_eq!(short_name_of("PC (Cam (wide))"), Some("Cam (wide)"));
    }
}
