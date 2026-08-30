use std::{env, net::SocketAddr, time::Duration};

use anyhow::Context;
use base64::{Engine as _, engine::general_purpose::STANDARD};

use crate::{auth::InternalAuth, crypto::SecretBox};

#[derive(Clone)]
pub struct ServeConfig {
    pub bind: SocketAddr,
    pub database_url: String,
    pub production: bool,
    pub internal_auth: InternalAuth,
    pub secrets: SecretBox,
    pub bootstrap_admin_email: String,
    pub bootstrap_admin_password: Option<String>,
}

impl ServeConfig {
    pub fn from_runtime() -> anyhow::Result<Self> {
        let database_url = required("SUNSHINE_MANAGER_DATABASE_URL")?;
        if !database_url.starts_with("sqlite:") && !database_url.starts_with("sqlite://") {
            anyhow::bail!("SUNSHINE_MANAGER_DATABASE_URL must be a SQLite URL");
        }

        let credential_key = decode_key(&required("SUNSHINE_MANAGER_CREDENTIAL_KEY")?)?;
        let bind: SocketAddr = value("SUNSHINE_MANAGER_BIND", "127.0.0.1:18104")
            .parse()
            .context("SUNSHINE_MANAGER_BIND must be a socket address")?;
        let production = parse_bool("SUNSHINE_MANAGER_PRODUCTION", true)?;
        let session_absolute_ttl =
            Duration::from_secs(parse_u64("SUNSHINE_MANAGER_SESSION_TTL_SECONDS", 43_200)?);
        let session_idle_ttl = Duration::from_secs(parse_u64(
            "SUNSHINE_MANAGER_SESSION_IDLE_TTL_SECONDS",
            1_800,
        )?);
        let cookie_secure = parse_bool("SUNSHINE_MANAGER_SESSION_COOKIE_SECURE", production)?;
        if production && !cookie_secure {
            anyhow::bail!("production requires Secure session cookies");
        }
        if !cookie_secure && !bind.ip().is_loopback() {
            anyhow::bail!("insecure development cookies require a loopback bind address");
        }

        Ok(Self {
            bind,
            database_url,
            production,
            internal_auth: InternalAuth::new(
                session_absolute_ttl,
                session_idle_ttl,
                cookie_secure,
            )?,
            secrets: SecretBox::new(
                value("SUNSHINE_MANAGER_CREDENTIAL_KEY_ID", "primary"),
                credential_key,
            )?,
            bootstrap_admin_email: value(
                "SUNSHINE_MANAGER_BOOTSTRAP_ADMIN_EMAIL",
                "admin@example.com",
            ),
            bootstrap_admin_password: env::var("SUNSHINE_MANAGER_BOOTSTRAP_ADMIN_PASSWORD").ok(),
        })
    }
}

fn decode_key(value: &str) -> anyhow::Result<[u8; 32]> {
    let decoded = STANDARD
        .decode(value.trim())
        .context("SUNSHINE_MANAGER_CREDENTIAL_KEY must be base64")?;
    decoded.try_into().map_err(|_| {
        anyhow::anyhow!("SUNSHINE_MANAGER_CREDENTIAL_KEY must decode to exactly 32 bytes")
    })
}

fn required(name: &str) -> anyhow::Result<String> {
    env::var(name).with_context(|| format!("{name} is required"))
}

fn value(name: &str, default: &str) -> String {
    env::var(name).unwrap_or_else(|_| default.to_string())
}

fn parse_u64(name: &str, default: u64) -> anyhow::Result<u64> {
    value(name, &default.to_string())
        .parse()
        .with_context(|| format!("{name} must be an unsigned integer"))
}

fn parse_bool(name: &str, default: bool) -> anyhow::Result<bool> {
    value(name, if default { "true" } else { "false" })
        .parse()
        .with_context(|| format!("{name} must be true or false"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credential_key_decoding_requires_exactly_32_bytes() {
        assert!(decode_key(&STANDARD.encode([7_u8; 32])).is_ok());
        assert!(decode_key(&STANDARD.encode([7_u8; 31])).is_err());
    }
}
