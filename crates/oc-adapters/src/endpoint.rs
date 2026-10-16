//! Captured provider endpoint authority, shared by generation and discovery.
use crate::provider::ProviderError;
use std::net::{IpAddr, SocketAddr};

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct EndpointBinding {
    base: reqwest::Url,
    trusted: bool,
    source: String,
}

impl std::fmt::Debug for EndpointBinding {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EndpointBinding")
            .field("trusted", &self.trusted)
            .finish_non_exhaustive()
    }
}

impl EndpointBinding {
    pub(crate) fn provenance(&self) -> String {
        crate::compaction::fingerprint(&(self.base.as_str(), self.trusted, &self.source))
    }
    pub(crate) fn admit(base: &str, trusted: bool, source: &str) -> Result<Self, ProviderError> {
        let mut base = parse(base)?;
        let path = base.path().trim_end_matches('/').to_owned();
        base.set_path(&path);
        let binding = Self {
            base,
            trusted,
            source: source.to_owned(),
        };
        if let Some(host) = binding.base.host_str()
            && let Ok(ip) = host.trim_matches(['[', ']']).parse::<IpAddr>()
            && !binding.allows(ip)
        {
            return Err(ProviderError::PrivateHost);
        }
        Ok(binding)
    }

    fn contains(&self, url: &reqwest::Url) -> bool {
        url.scheme() == self.base.scheme()
            && url.host() == self.base.host()
            && url.port_or_known_default() == self.base.port_or_known_default()
            && (url.path() == self.base.path()
                || url
                    .path()
                    .strip_prefix(self.base.path())
                    .is_some_and(|p| p.starts_with('/'))
                || self.base.path() == "/")
    }

    pub(crate) fn allows(&self, ip: IpAddr) -> bool {
        allowed(ip, self.trusted, false)
    }
}

fn parse(url: &str) -> Result<reqwest::Url, ProviderError> {
    let url = reqwest::Url::parse(url).map_err(|_| ProviderError::InvalidConfig)?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(ProviderError::InvalidConfig);
    }
    Ok(url)
}

fn allowed(ip: IpAddr, trusted: bool, test_loopback: bool) -> bool {
    // Mapped IPv4 has exactly the same authority as its IPv4 counterpart.
    if let IpAddr::V6(ip) = ip
        && let Some(v4) = ip.to_ipv4_mapped()
    {
        return allowed(IpAddr::V4(v4), trusted, test_loopback);
    }
    if crate::webfetch::ip_is_public(ip) {
        return true;
    }
    if (trusted || test_loopback) && ip.is_loopback() {
        return true;
    }
    trusted
        && match ip {
            IpAddr::V4(ip) => ip.is_private(),
            IpAddr::V6(ip) => ip.is_unique_local(),
        }
}

pub(crate) fn peer_allowed(
    binding: Option<&EndpointBinding>,
    ip: IpAddr,
    test_loopback: bool,
) -> bool {
    allowed(ip, binding.is_some_and(|b| b.trusted), test_loopback)
}

/// Resolve once, validate every answer, then pin these answers in the HTTP client.
/// This closes the preflight/post-dial DNS gap before any credential is sent.
pub(crate) async fn resolve(
    url: &str,
    binding: Option<&EndpointBinding>,
    test_loopback: bool,
) -> Result<(String, Vec<SocketAddr>), ProviderError> {
    let url = parse(url)?;
    if binding.is_some_and(|b| !b.contains(&url)) {
        return Err(ProviderError::InvalidConfig);
    }
    let host = url
        .host_str()
        .ok_or(ProviderError::InvalidConfig)?
        .trim_matches(['[', ']']);
    let port = url
        .port_or_known_default()
        .ok_or(ProviderError::InvalidConfig)?;
    let addrs: Vec<_> = tokio::net::lookup_host((host, port))
        .await
        .map_err(|_| ProviderError::PrivateHost)?
        .collect();
    if addrs.is_empty()
        || addrs
            .iter()
            .any(|a| !peer_allowed(binding, a.ip(), test_loopback))
    {
        return Err(ProviderError::PrivateHost);
    }
    Ok((host.to_owned(), addrs))
}

#[cfg(test)]
#[path = "endpoint/tests.rs"]
mod tests;
