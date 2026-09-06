//! Product-owned management protocol. No streaming, arbitrary command, path or URL messages.
use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

pub mod config;

pub const PROTOCOL: &str = "sunshine-management/1";
/// A slash is not legal in an RFC 6455 WebSocket subprotocol header token.
pub const WEBSOCKET_SUBPROTOCOL: &str = "sunshine-management.v1";
pub const SUNSHINE_VERSION: &str = "2026.516.143833";
pub const MAX_MESSAGE_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Binding {
    pub manager_id: Uuid,
    pub device_id: Uuid,
    pub installation_id: Uuid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Permission {
    ReadConfig,
    WriteConfig,
    Restart,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RestartPolicy {
    Manual,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
    ReadConfig {},
    PatchConfig {
        expected_revision: String,
        set: BTreeMap<String, config::FieldValue>,
        remove: BTreeSet<String>,
        restart_policy: RestartPolicy,
    },
    Restart {
        expected_revision: String,
        /// An explicit administrator decision; the Agent must ALSO allow restart locally.
        administrator_confirmed: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Task {
    pub protocol: String,
    pub operation_id: String,
    pub binding: Binding,
    pub permission: Permission,
    pub command: Command,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeliveryMode {
    Execute,
    /// May inspect durable facts/actual state, but MUST NOT perform a side effect.
    InspectOnly,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum ManagerMessage {
    Task { mode: DeliveryMode, task: Task },
    Revoked {},
}

/// These are execution observations, NOT another persistent task lifecycle.
/// Manager maps observations into the existing Foundation operation transitions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Report {
    ConfigRead { snapshot: ConfigSnapshot },
    ConfigSaved { snapshot: ConfigSnapshot },
    RestartAcknowledged { snapshot: ConfigSnapshot },
    Conflict { actual_revision: String },
    Rejected { reason: Rejection },
    Unknown { reason: Uncertainty },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Rejection {
    InvalidTask,
    BindingMismatch,
    PermissionDenied,
    OperationIdReused,
    UnsupportedVersion,
    UnsafeConfiguration,
    SunshineUnavailable,
    JournalFull,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Uncertainty {
    NoExecutionRecord,
    EffectNotConfirmed,
    RestartNotConfirmed,
    PersistenceFailure,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Effectiveness {
    /// Reading a file does not prove that its settings are used by the running process.
    PendingVerification,
    AwaitingRestart,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConfigSnapshot {
    pub revision: String,
    pub sunshine_version: String,
    /// Only allowlisted fields; never send the entire local configuration to Manager.
    pub fields: BTreeMap<String, String>,
    pub effectiveness: Effectiveness,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Capabilities {
    pub protocol: String,
    pub agent_version: String,
    pub os: AgentOs,
    pub sunshine_version: String,
    pub restart_allowed: bool,
    pub managed_fields: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentOs {
    LinuxX86_64,
    WindowsX86_64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum AgentMessage {
    Hello {
        binding: Binding,
        capabilities: Capabilities,
    },
    Heartbeat {
        sunshine_reachable: Option<bool>,
        configuration: Option<ConfigSnapshot>,
    },
    Result {
        operation_id: String,
        report: Report,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("invalid sunshine management task")]
pub struct InvalidTask;

impl Task {
    pub fn validate(&self, binding: &Binding, restart_allowed: bool) -> Result<(), Rejection> {
        if &self.binding != binding {
            return Err(Rejection::BindingMismatch);
        }
        if self.protocol != PROTOCOL
            || self.binding.manager_id.is_nil()
            || self.binding.device_id.is_nil()
            || self.binding.installation_id.is_nil()
            || self.operation_id.strip_prefix("op_").is_none_or(|id| {
                !Uuid::parse_str(id).is_ok_and(|uuid| !uuid.is_nil() && uuid.to_string() == id)
            })
        {
            return Err(Rejection::InvalidTask);
        }
        let permission = match &self.command {
            Command::ReadConfig {} => Permission::ReadConfig,
            Command::PatchConfig {
                expected_revision,
                set,
                remove,
                ..
            } => {
                validate_revision(expected_revision).map_err(|_| Rejection::InvalidTask)?;
                if set.is_empty() && remove.is_empty()
                    || set.len() + remove.len() > config::FIELDS.len()
                {
                    return Err(Rejection::InvalidTask);
                }
                for (key, value) in set {
                    if remove.contains(key) || config::validate_field(key, value).is_err() {
                        return Err(Rejection::InvalidTask);
                    }
                }
                if remove
                    .iter()
                    .any(|key| !config::FIELDS.contains(&key.as_str()))
                {
                    return Err(Rejection::InvalidTask);
                }
                Permission::WriteConfig
            }
            Command::Restart {
                expected_revision,
                administrator_confirmed,
            } => {
                validate_revision(expected_revision).map_err(|_| Rejection::InvalidTask)?;
                if !restart_allowed || !administrator_confirmed {
                    return Err(Rejection::PermissionDenied);
                }
                Permission::Restart
            }
        };
        if self.permission != permission {
            return Err(Rejection::PermissionDenied);
        }
        Ok(())
    }

    /// Stable across object/set ordering and retransmission mode. Binding and permission are covered.
    pub fn fingerprint(&self) -> Result<String, InvalidTask> {
        let bytes = serde_json::to_vec(self).map_err(|_| InvalidTask)?;
        if bytes.len() > MAX_MESSAGE_BYTES {
            return Err(InvalidTask);
        }
        Ok(format!("{:x}", Sha256::digest(bytes)))
    }
}

pub fn validate_revision(revision: &str) -> Result<(), InvalidTask> {
    if revision.len() == 64
        && revision
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        Ok(())
    } else {
        Err(InvalidTask)
    }
}

pub fn decode_manager_message(bytes: &[u8]) -> Result<ManagerMessage, InvalidTask> {
    if bytes.len() > MAX_MESSAGE_BYTES {
        return Err(InvalidTask);
    }
    serde_json::from_slice(bytes).map_err(|_| InvalidTask)
}
