use std::{
    collections::{HashMap, HashSet},
    net::IpAddr,
    sync::{Arc, Mutex, MutexGuard},
    time::{Duration, Instant},
};

use anyhow::ensure;
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use rand::RngCore;
use reqwest::Url;

use crate::{
    cover_policy::VerifiedCover,
    error::{AppError, AppResult},
    model::Host,
    release_contract::{API_NAMESPACE, API_VERSION_PREFIX},
};

const TOKEN_BYTES: usize = 32;
const TOKEN_LENGTH: usize = 43;
const ENTRY_TTL: Duration = Duration::from_secs(30);
const MAX_ENTRIES: usize = 16;
const MAX_HOST_ADDRESSES: usize = 16;

#[derive(Clone)]
pub struct CoverProxy {
    origin: Option<Url>,
    entries: Arc<Mutex<HashMap<String, CoverEntry>>>,
}

struct CoverEntry {
    host_id: String,
    operation_id: String,
    allowed_sources: HashSet<IpAddr>,
    expires_at: Instant,
    cover: VerifiedCover,
}

impl CoverProxy {
    pub fn from_origin(raw_origin: &str) -> anyhow::Result<Self> {
        let origin = Url::parse(raw_origin.trim())?;
        ensure!(
            origin.scheme() == "https"
                && origin.host_str().is_some()
                && origin.username().is_empty()
                && origin.password().is_none()
                && origin.path() == "/"
                && origin.query().is_none()
                && origin.fragment().is_none(),
            "SUNSHINE_MANAGER_COVER_PROXY_ORIGIN must be an HTTPS origin without credentials, path, query or fragment"
        );
        Ok(Self {
            origin: Some(origin),
            entries: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    pub fn disabled() -> Self {
        Self {
            origin: None,
            entries: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn publish(
        &self,
        host: &Host,
        operation_id: &str,
        cover: VerifiedCover,
    ) -> AppResult<String> {
        let origin = self.origin.as_ref().ok_or_else(|| {
            AppError::Internal(anyhow::anyhow!("cover proxy origin is unavailable"))
        })?;
        let allowed_sources = resolve_host_sources(host).await?;
        let token = random_token();
        let relative = format!(
            "{}{}/sunshine/internal/hosts/{}/operations/{}/covers/{}",
            API_NAMESPACE, API_VERSION_PREFIX, host.id, operation_id, token
        );
        let url = origin
            .join(relative.trim_start_matches('/'))
            .map_err(|error| AppError::Internal(error.into()))?
            .to_string();
        let now = Instant::now();
        let mut entries = recover_lock(&self.entries);
        entries.retain(|_, entry| entry.expires_at > now);
        if entries.len() >= MAX_ENTRIES {
            return Err(AppError::Upstream(
                "cover delivery capacity is exhausted".into(),
            ));
        }
        entries.insert(
            token.clone(),
            CoverEntry {
                host_id: host.id.clone(),
                operation_id: operation_id.to_string(),
                allowed_sources,
                expires_at: now + ENTRY_TTL,
                cover,
            },
        );
        drop(entries);
        Ok(url)
    }

    pub fn take(
        &self,
        host_id: &str,
        operation_id: &str,
        token: &str,
        peer: IpAddr,
    ) -> AppResult<VerifiedCover> {
        if token.len() != TOKEN_LENGTH
            || !token
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        {
            return Err(not_found());
        }
        let now = Instant::now();
        let peer = canonical_ip(peer);
        let mut entries = recover_lock(&self.entries);
        entries.retain(|_, entry| entry.expires_at > now);
        let matches = entries.get(token).is_some_and(|entry| {
            entry.host_id == host_id
                && entry.operation_id == operation_id
                && entry.allowed_sources.contains(&peer)
        });
        if !matches {
            return Err(not_found());
        }
        entries
            .remove(token)
            .map(|entry| entry.cover)
            .ok_or_else(not_found)
    }
}

async fn resolve_host_sources(host: &Host) -> AppResult<HashSet<IpAddr>> {
    let addresses: HashSet<IpAddr> = tokio::net::lookup_host((host.host.as_str(), host.web_port))
        .await
        .map_err(|_| AppError::Upstream("Sunshine host could not be resolved".into()))?
        .map(|address| canonical_ip(address.ip()))
        .collect();
    if addresses.is_empty() || addresses.len() > MAX_HOST_ADDRESSES {
        return Err(AppError::Upstream(
            "Sunshine host resolved to an unsafe address set".into(),
        ));
    }
    Ok(addresses)
}

fn canonical_ip(address: IpAddr) -> IpAddr {
    match address {
        IpAddr::V6(address) => address
            .to_ipv4_mapped()
            .map(IpAddr::V4)
            .unwrap_or(IpAddr::V6(address)),
        address => address,
    }
}

fn random_token() -> String {
    let mut bytes = [0_u8; TOKEN_BYTES];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

fn not_found() -> AppError {
    AppError::NotFound("cover delivery not found".into())
}

fn recover_lock<T>(lock: &Mutex<T>) -> MutexGuard<'_, T> {
    lock.lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn host() -> Host {
        Host {
            id: "host-1".into(),
            name: "Sunshine".into(),
            host: "127.0.0.1".into(),
            web_port: 47990,
            username: "admin".into(),
            password: "secret".into(),
            verify_tls: true,
            position: 0,
            created_at_micros: 1,
            updated_at_micros: 1,
        }
    }

    fn cover() -> VerifiedCover {
        VerifiedCover {
            content_type: "image/png",
            bytes: b"image".to_vec(),
        }
    }

    #[tokio::test]
    async fn delivery_is_bound_to_host_operation_source_and_one_use() {
        let proxy = CoverProxy::from_origin("https://manager.internal/").unwrap();
        let url = proxy
            .publish(&host(), "operation-1", cover())
            .await
            .unwrap();
        let token = url.rsplit('/').next().unwrap();
        assert!(
            proxy
                .take("host-1", "operation-1", token, "127.0.0.2".parse().unwrap())
                .is_err()
        );
        assert!(
            proxy
                .take(
                    "host-1",
                    "wrong-operation",
                    token,
                    "127.0.0.1".parse().unwrap()
                )
                .is_err()
        );
        assert_eq!(
            proxy
                .take(
                    "host-1",
                    "operation-1",
                    token,
                    "::ffff:127.0.0.1".parse().unwrap()
                )
                .unwrap()
                .bytes,
            b"image"
        );
        assert!(
            proxy
                .take("host-1", "operation-1", token, "127.0.0.1".parse().unwrap())
                .is_err()
        );
    }

    #[test]
    fn origin_is_an_exact_https_origin() {
        assert!(CoverProxy::from_origin("https://manager.internal/").is_ok());
        for invalid in [
            "http://manager.internal/",
            "https://user@manager.internal/",
            "https://manager.internal/path",
            "https://manager.internal/?query=1",
        ] {
            assert!(CoverProxy::from_origin(invalid).is_err(), "{invalid}");
        }
    }
}
