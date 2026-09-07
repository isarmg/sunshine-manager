use crate::error::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use sunshine_client_protocol::{Capabilities, ConfigSnapshot};

#[derive(Debug, Serialize)]
pub struct DeviceView {
    pub id: String,
    pub name: String,
    pub registered: bool,
    pub pairing_pending: bool,
    pub revoked: bool,
    pub client_online: bool,
    pub sunshine_reachable: Option<bool>,
    pub configuration_state: String,
    pub snapshot: Option<ConfigSnapshot>,
    pub capabilities: Option<Capabilities>,
    pub last_seen_at_micros: Option<i64>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeviceName {
    pub name: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OperationResolutionRequest {
    pub resolution: sarmg_operations::Resolution,
}
pub fn validate_instance_name(value: &str) -> AppResult<()> {
    if value.trim().is_empty()
        || value != value.trim()
        || value.chars().count() > 32
        || value.chars().any(char::is_control)
    {
        return Err(AppError::BadRequest(
            "实例名称必须为 1–32 个字符，不能包含控制字符或首尾空格".into(),
        ));
    }
    Ok(())
}
