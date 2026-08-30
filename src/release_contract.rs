use anyhow::{Context, ensure};
use serde::{Deserialize, Serialize};

use crate::database_schema::{APPLICATION, APPLICATION_VERSION, SCHEMA_REVISION, SCHEMA_SHA256};

pub const MANIFEST_FORMAT: &str = "sunshine-manager-release-v1";
pub const API_NAMESPACE: &str = "/api";
pub const API_VERSION_PREFIX: &str = "/v2";
pub const API_PREFIX: &str = "/api/v2";
pub const BUILD_TARGET: &str = env!("SUNSHINE_MANAGER_BUILD_TARGET");
pub const SOURCE_REVISION: &str = env!("SUNSHINE_MANAGER_SOURCE_REVISION");

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReleaseContract {
    pub manifest_format: String,
    pub application: String,
    pub version: String,
    pub api_prefix: String,
    pub schema_revision: i64,
    pub schema_sha256: String,
    pub target: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BinaryIdentity {
    pub manifest_format: String,
    pub application: String,
    pub version: String,
    pub api_prefix: String,
    pub schema_revision: i64,
    pub schema_sha256: String,
    pub target: String,
    pub source_revision: String,
}

impl ReleaseContract {
    pub fn current() -> Self {
        Self {
            manifest_format: MANIFEST_FORMAT.to_owned(),
            application: APPLICATION.to_owned(),
            version: APPLICATION_VERSION.to_owned(),
            api_prefix: API_PREFIX.to_owned(),
            schema_revision: SCHEMA_REVISION,
            schema_sha256: SCHEMA_SHA256.to_owned(),
            target: BUILD_TARGET.to_owned(),
        }
    }
}

impl BinaryIdentity {
    pub fn current() -> anyhow::Result<Self> {
        let contract = embedded()?;
        Ok(Self {
            manifest_format: contract.manifest_format,
            application: contract.application,
            version: contract.version,
            api_prefix: contract.api_prefix,
            schema_revision: contract.schema_revision,
            schema_sha256: contract.schema_sha256,
            target: contract.target,
            source_revision: SOURCE_REVISION.to_owned(),
        })
    }

    pub fn is_release_bound(&self) -> bool {
        self.source_revision.len() == 40
            && self
                .source_revision
                .bytes()
                .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
    }
}

pub fn parse_exact(input: &str) -> anyhow::Result<ReleaseContract> {
    let parsed: ReleaseContract =
        serde_json::from_str(input).context("release contract must be strict JSON")?;
    ensure!(
        parsed == ReleaseContract::current(),
        "release contract is not the exact compiled Sunshine Manager identity"
    );
    Ok(parsed)
}

pub fn embedded() -> anyhow::Result<ReleaseContract> {
    parse_exact(include_str!("../release.json"))
}

pub fn current_json() -> anyhow::Result<String> {
    serde_json::to_string(&BinaryIdentity::current()?).context("serialize current binary identity")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_contract_is_the_exact_compiled_identity() {
        assert_eq!(embedded().unwrap(), ReleaseContract::current());
    }

    #[test]
    fn unknown_fields_and_other_versions_are_rejected() {
        let mut unknown: serde_json::Value =
            serde_json::from_str(include_str!("../release.json")).unwrap();
        unknown["compatibility"] = serde_json::json!(true);
        assert!(parse_exact(&unknown.to_string()).is_err());

        let mut other = ReleaseContract::current();
        other.version = "0.6.0".to_owned();
        assert!(parse_exact(&serde_json::to_string(&other).unwrap()).is_err());
    }

    #[test]
    fn ordinary_development_identity_is_explicitly_unbound() {
        let identity = BinaryIdentity::current().unwrap();
        assert_eq!(identity.source_revision, SOURCE_REVISION);
        assert_eq!(identity.is_release_bound(), SOURCE_REVISION != "unbound");
    }
}
