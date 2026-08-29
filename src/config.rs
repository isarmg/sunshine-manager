use std::net::SocketAddr;

use anyhow::Context;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde::Deserialize;

use crate::{auth::InternalAuth, crypto::SecretBox};

#[derive(Clone)]
pub struct ServeConfig {
    pub bind: SocketAddr,
    pub database_url: String,
    pub production: bool,
    pub internal_auth: InternalAuth,
    pub secrets: SecretBox,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RuntimeConfiguration {
    database_url: String,
    credential_key: String,
    #[serde(default = "default_credential_key_id")]
    credential_key_id: String,
    #[serde(default = "default_production")]
    production: bool,
}

impl ServeConfig {
    pub fn from_runtime() -> anyhow::Result<Self> {
        let manifest =
            sarmg_platform_core::PluginManifest::parse_json(include_str!("../manifest.json"))?;
        let context = sarmg_platform_sdk::ProcessContext::from_env(&manifest)?;
        let configuration: RuntimeConfiguration = context.load_configuration()?;
        if !configuration.database_url.starts_with("postgresql://")
            && !configuration.database_url.starts_with("postgres://")
        {
            anyhow::bail!("Sunshine requires a PostgreSQL database URL");
        }
        let credential_key = decode_key(&configuration.credential_key)?;
        Ok(Self {
            bind: context.bind,
            database_url: configuration.database_url,
            production: configuration.production,
            internal_auth: InternalAuth::from_env(crate::auth::AUDIENCE, crate::auth::PREFIX)?,
            secrets: SecretBox::new(configuration.credential_key_id, credential_key)?,
        })
    }
}

fn decode_key(value: &str) -> anyhow::Result<[u8; 32]> {
    let decoded = STANDARD
        .decode(value.trim())
        .context("credential_key must be base64")?;
    decoded
        .try_into()
        .map_err(|_| anyhow::anyhow!("credential_key must decode to exactly 32 bytes"))
}

fn default_credential_key_id() -> String {
    "primary".into()
}

const fn default_production() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::{AUDIENCE, PREFIX, PROTOCOL};

    #[test]
    fn configuration_defaults_are_current_only() {
        let configuration: RuntimeConfiguration = serde_json::from_value(serde_json::json!({
            "database_url": "postgresql://localhost/sunshine",
            "credential_key": STANDARD.encode([7_u8; 32])
        }))
        .unwrap();
        assert!(configuration.production);
        assert_eq!(configuration.credential_key_id, "primary");
        assert_eq!(decode_key(&configuration.credential_key).unwrap(), [7; 32]);
    }

    #[test]
    fn compiled_gateway_identity_is_fixed() {
        assert_eq!(PROTOCOL, "gateway-v1");
        assert_eq!(AUDIENCE, "sunshine");
        assert_eq!(PREFIX, "/api/modules/sunshine");
    }
}
