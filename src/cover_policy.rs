use std::{
    collections::HashSet,
    net::{IpAddr, SocketAddr},
    sync::Arc,
    time::Duration,
};

use anyhow::{Context, ensure};
use reqwest::{Url, header};

use crate::error::{AppError, AppResult};

const MAX_URL_BYTES: usize = 4 * 1024;
const MAX_DNS_ADDRESSES: usize = 16;
const MAX_COVER_BYTES: usize = 8 * 1024 * 1024;
const ALLOWED_MEDIA_TYPES: [&str; 5] = [
    "image/jpeg",
    "image/png",
    "image/webp",
    "image/gif",
    "image/avif",
];

pub struct VerifiedCover {
    pub content_type: &'static str,
    pub bytes: Vec<u8>,
}

/// Policy for external covers fetched by Manager. Host names are exact
/// matches: wildcards are intentionally unsupported.
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
        let (url, _) = self.resolve(raw_url).await?;
        Ok(url.to_string())
    }

    pub async fn download(&self, raw_url: &str) -> AppResult<VerifiedCover> {
        let (url, addresses) = self.resolve(raw_url).await?;
        let host = url.host_str().ok_or_else(rejected)?;
        let sockets: Vec<SocketAddr> = addresses
            .into_iter()
            .map(|address| SocketAddr::new(address, 443))
            .collect();
        // Pinning the approved socket set does not replace TLS identity
        // verification: the URL host remains the SNI/certificate name and the
        // platform verifier stays mandatory. No insecure development path is
        // supported here either.
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(3))
            .timeout(Duration::from_secs(15))
            .redirect(reqwest::redirect::Policy::none())
            .resolve_to_addrs(host, &sockets)
            .build()
            .map_err(|error| AppError::Internal(error.into()))?;
        let mut response = client
            .get(url)
            .send()
            .await
            .map_err(|_| AppError::Upstream("approved cover download failed".into()))?;
        if !response.status().is_success() {
            return Err(AppError::Upstream(
                "approved cover download returned a non-success status".into(),
            ));
        }
        if response
            .content_length()
            .is_some_and(|length| length > MAX_COVER_BYTES as u64)
        {
            return Err(AppError::Upstream("approved cover is too large".into()));
        }
        let media_type = response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .and_then(allowed_media_type)
            .ok_or_else(|| {
                AppError::Upstream("approved cover has an unsupported media type".into())
            })?;
        let mut bytes = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| AppError::Upstream("approved cover download was interrupted".into()))?
        {
            if bytes.len().saturating_add(chunk.len()) > MAX_COVER_BYTES {
                return Err(AppError::Upstream("approved cover is too large".into()));
            }
            bytes.extend_from_slice(&chunk);
        }
        if bytes.is_empty() {
            return Err(AppError::Upstream("approved cover is empty".into()));
        }
        Ok(VerifiedCover {
            content_type: media_type,
            bytes,
        })
    }

    pub fn is_empty(&self) -> bool {
        self.allowed_hosts.is_empty()
    }

    async fn resolve(&self, raw_url: &str) -> AppResult<(Url, HashSet<IpAddr>)> {
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
        Ok((url, addresses))
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

fn allowed_media_type(value: &str) -> Option<&'static str> {
    let candidate = value.split(';').next()?.trim();
    ALLOWED_MEDIA_TYPES
        .iter()
        .copied()
        .find(|allowed| candidate.eq_ignore_ascii_case(allowed))
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

    #[test]
    fn cover_media_type_is_an_exact_image_allowlist() {
        assert_eq!(allowed_media_type("image/png"), Some("image/png"));
        assert_eq!(
            allowed_media_type("IMAGE/JPEG; charset=binary"),
            Some("image/jpeg")
        );
        assert_eq!(allowed_media_type("image/svg+xml"), None);
        assert_eq!(allowed_media_type("text/html"), None);
    }
}
