use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Host {
    pub id: String,
    pub name: String,
    pub host: String,
    pub web_port: u16,
    pub username: String,
    #[serde(skip_serializing)]
    pub password: String,
    pub position: i64,
    pub created_at_micros: i64,
    pub updated_at_micros: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProbeStatus {
    Pending,
    Complete,
}

#[derive(Debug, Clone, Serialize)]
pub struct HostInfo {
    pub id: String,
    pub name: String,
    pub host: String,
    pub web_port: u16,
    pub username: String,
    pub password_set: bool,
    pub web_url: String,
    pub probe_status: ProbeStatus,
    pub reachable: Option<bool>,
    pub connected: Option<bool>,
    pub connection_error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct HostStatus {
    pub host: String,
    pub web_port: u16,
    pub web_url: String,
    pub reachable: bool,
    pub message: String,
}

#[derive(Debug, Clone, Default)]
pub struct HealthSnapshot {
    pub reachable: Option<bool>,
    pub connected: Option<bool>,
    pub connection_error: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HostSaveRequest {
    pub name: String,
    pub host: String,
    pub web_port: u16,
    pub username: String,
    pub password: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HostPatchRequest {
    pub name: Option<String>,
    pub host: Option<String>,
    pub web_port: Option<u16>,
    pub username: Option<String>,
    pub password: Option<String>,
}

impl HostPatchRequest {
    pub fn is_empty(&self) -> bool {
        self.name.is_none()
            && self.host.is_none()
            && self.web_port.is_none()
            && self.username.is_none()
            && self.password.is_none()
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClientUpdateRequest {
    pub uuid: String,
    pub enabled: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UnpairRequest {
    pub uuid: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PinRequest {
    pub pin: String,
    pub name: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CoverUploadRequest {
    pub key: String,
    pub url: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OperationResolutionRequest {
    pub resolution: OperationResolution,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationResolution {
    ConfirmedSucceeded,
    ConfirmedFailed,
}

impl OperationResolution {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ConfirmedSucceeded => "confirmed_succeeded",
            Self::ConfirmedFailed => "confirmed_failed",
        }
    }
}

pub fn validate_host_request(request: &HostSaveRequest) -> AppResult<()> {
    validate_text("host name", &request.name, 128)?;
    validate_text("username", &request.username, 256)?;
    if request
        .password
        .as_ref()
        .is_some_and(|password| password.len() > 4096)
    {
        return Err(AppError::BadRequest(
            "Sunshine password cannot exceed 4096 bytes".to_string(),
        ));
    }
    if request.web_port == 0 {
        return Err(AppError::BadRequest(
            "web_port must be non-zero".to_string(),
        ));
    }
    if !is_valid_host(&request.host) {
        return Err(AppError::BadRequest("invalid Sunshine host".to_string()));
    }
    Ok(())
}

pub fn normalize_host(value: &str) -> String {
    value.trim().trim_matches(['[', ']']).to_string()
}

pub fn url_host(value: &str) -> String {
    let host = normalize_host(value);
    if host.parse::<std::net::Ipv6Addr>().is_ok() {
        format!("[{host}]")
    } else {
        host
    }
}

pub fn web_url(host: &Host) -> String {
    format!("https://{}:{}", url_host(&host.host), host.web_port)
}

fn is_valid_host(value: &str) -> bool {
    let host = normalize_host(value);
    if host.parse::<std::net::IpAddr>().is_ok() {
        return true;
    }
    let name = host.trim_end_matches('.');
    !name.is_empty()
        && name.len() <= 253
        && name.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && label
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
                && !label.starts_with('-')
                && !label.ends_with('-')
        })
}

fn validate_text(label: &str, value: &str, limit: usize) -> AppResult<()> {
    let value = value.trim();
    if value.is_empty() || value.chars().count() > limit || value.chars().any(char::is_control) {
        return Err(AppError::BadRequest(format!("invalid {label}")));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_invalid_hosts() {
        let mut request = HostSaveRequest {
            name: "desktop".into(),
            host: "2001:db8::1".into(),
            web_port: 47990,
            username: "admin".into(),
            password: Some("secret".into()),
        };
        assert!(validate_host_request(&request).is_ok());
        request.host = "not a host".into();
        assert!(validate_host_request(&request).is_err());
    }

    #[test]
    fn host_contract_rejects_unknown_fields() {
        let current = serde_json::json!({
            "name": "desktop",
            "host": "sunshine.example.com",
            "web_port": 47990,
            "username": "admin",
            "password": "secret"
        });
        assert!(serde_json::from_value::<HostSaveRequest>(current.clone()).is_ok());

        let mut invalid = current.clone();
        invalid["unexpected"] = serde_json::Value::Bool(false);
        assert!(serde_json::from_value::<HostSaveRequest>(invalid).is_err());

        let mut forbidden_tls_bypass = current;
        forbidden_tls_bypass["verify_tls"] = serde_json::Value::Bool(false);
        assert!(serde_json::from_value::<HostSaveRequest>(forbidden_tls_bypass).is_err());
    }
}
