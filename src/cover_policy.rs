use std::{collections::HashSet, net::IpAddr, sync::Arc};

use anyhow::{Context, ensure};
use reqwest::Url;

use crate::error::{AppError, AppResult};

const MAX_URL_BYTES: usize = 4 * 1024;
const MAX_DNS_ADDRESSES: usize = 16;

/// Policy for URLs that a managed Sunshine host will fetch. Host names are
/// exact matches: wildcards are intentionally unsupported.
#[derive(Clone, Debug, Default)]
pub struct CoverUrlPolicy {
    allowed_hosts: Arc<HashSet<String>>,
}

impl CoverUrlPolicy {
    pub fn from_csv(value: &str) -> anyhow::Result<Self> {
        let mut allowed_hosts = HashSet::new();
        for entry in value
            .split(',')
            .map(str::trim)
            .filter(|item| !item.is_empty())
        {
            ensure!(
                entry.is_ascii() && entry == entry.to_ascii_lowercase(),
                "SUNSHINE_MANAGER_COVER_URL_ALLOWLIST entries must be lowercase ASCII host names"
            );
            ensure!(
                !entry.ends_with('.')
                    && !['/', ':', '@', '*']
                        .iter()
                        .any(|character| entry.contains(*character)),
                "SUNSHINE_MANAGER_COVER_URL_ALLOWLIST accepts exact host names only"
            );
            let parsed = Url::parse(&format!("https://{entry}/"))
                .context("invalid SUNSHINE_MANAGER_COVER_URL_ALLOWLIST host")?;
            ensure!(
                parsed.host_str() == Some(entry) && entry.parse::<IpAddr>().is_err(),
                "SUNSHINE_MANAGER_COVER_URL_ALLOWLIST accepts DNS host names only"
            );
            allowed_hosts.insert(entry.to_string());
        }
        Ok(Self {
            allowed_hosts: Arc::new(allowed_hosts),
        })
    }

    pub async fn validate(&self, raw_url: &str) -> AppResult<String> {
        let raw_url = raw_url.trim();
        if raw_url.is_empty() || raw_url.len() > MAX_URL_BYTES {
            return Err(rejected());
        }
        let url = Url::parse(raw_url).map_err(|_| rejected())?;
        let host = url.host_str().ok_or_else(rejected)?.to_ascii_lowercase();
        if url.scheme() != "https"
            || host.ends_with('.')
            || host.parse::<IpAddr>().is_ok()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.fragment().is_some()
            || url.port_or_known_default() != Some(443)
            || !self.allowed_hosts.contains(&host)
        {
            return Err(rejected());
        }

        let addresses: HashSet<IpAddr> = tokio::net::lookup_host((host.as_str(), 443))
            .await
            .map_err(|_| AppError::Upstream("approved cover host could not be resolved".into()))?
            .map(|address| address.ip())
            .collect();
        if addresses.is_empty()
            || addresses.len() > MAX_DNS_ADDRESSES
            || addresses.iter().any(|address| !is_public(*address))
        {
            return Err(rejected());
        }
        Ok(url.to_string())
    }

    #[cfg(test)]
    fn allows_structure(&self, raw_url: &str) -> bool {
        let Ok(url) = Url::parse(raw_url) else {
            return false;
        };
        let Some(host) = url.host_str() else {
            return false;
        };
        !host.ends_with('.')
            && host.parse::<IpAddr>().is_err()
            && url.scheme() == "https"
            && url.username().is_empty()
            && url.password().is_none()
            && url.fragment().is_none()
            && url.port_or_known_default() == Some(443)
            && self.allowed_hosts.contains(&host.to_ascii_lowercase())
    }
}

fn rejected() -> AppError {
    AppError::BadRequest("cover URL is not permitted by the outbound policy".into())
}

fn is_public(address: IpAddr) -> bool {
    match address {
        IpAddr::V4(address) => {
            let [a, b, c, _] = address.octets();
            !(a == 0
                || a == 10
                || a == 127
                || (a == 100 && (64..=127).contains(&b))
                || (a == 169 && b == 254)
                || (a == 172 && (16..=31).contains(&b))
                || (a == 192 && b == 0)
                || (a == 192 && b == 88 && c == 99)
                || (a == 192 && b == 168)
                || (a == 192 && b == 0 && c == 2)
                || (a == 198 && (b == 18 || b == 19))
                || (a == 198 && b == 51 && c == 100)
                || (a == 203 && b == 0 && c == 113)
                || a >= 224)
        }
        IpAddr::V6(address) => {
            if let Some(address) = address.to_ipv4_mapped() {
                return is_public(IpAddr::V4(address));
            }
            let octets = address.octets();
            !address.is_unspecified()
                && !address.is_loopback()
                && octets[0] != 0xff
                && octets[0] & 0xfe != 0xfc
                && !(octets[0] == 0xfe && octets[1] & 0xc0 == 0x80)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allowlist_is_exact_and_url_structure_is_strict() {
        let policy = CoverUrlPolicy::from_csv("covers.example.com,cdn.example.com").unwrap();
        assert!(policy.allows_structure("https://covers.example.com/image.jpg?token=1"));
        assert!(policy.allows_structure("https://covers.example.com:443/image.jpg"));
        assert!(!policy.allows_structure("http://covers.example.com/image.jpg"));
        assert!(!policy.allows_structure("https://sub.covers.example.com/image.jpg"));
        assert!(!policy.allows_structure("https://covers.example.com./image.jpg"));
        assert!(!policy.allows_structure("https://user@covers.example.com/image.jpg"));
        assert!(!policy.allows_structure("https://covers.example.com/image.jpg#fragment"));
        assert!(!policy.allows_structure("https://127.0.0.1/image.jpg"));
        assert!(CoverUrlPolicy::from_csv("*.example.com").is_err());
        assert!(CoverUrlPolicy::from_csv("127.0.0.1").is_err());
    }

    #[test]
    fn dangerous_and_non_routable_addresses_are_rejected() {
        for address in [
            "0.0.0.0",
            "10.0.0.1",
            "100.100.100.200",
            "127.0.0.1",
            "169.254.169.254",
            "172.16.0.1",
            "192.168.1.1",
            "198.18.0.1",
            "224.0.0.1",
            "255.255.255.255",
            "::",
            "::1",
            "fc00::1",
            "fe80::1",
            "ff02::1",
            "::ffff:127.0.0.1",
        ] {
            assert!(!is_public(address.parse().unwrap()), "{address}");
        }
        assert!(is_public("1.1.1.1".parse().unwrap()));
        assert!(is_public("2606:4700:4700::1111".parse().unwrap()));
    }
}
