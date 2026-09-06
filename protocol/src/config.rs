//! Intentionally conservative v1 field policy. Omitted fields cannot be changed or deleted remotely.
use crate::InvalidTask;
use serde::{Deserialize, Serialize};

pub const FIELDS: &[&str] = &[
    "sunshine_name",
    "min_log_level",
    "qp",
    "hevc_mode",
    "av1_mode",
    "min_threads",
    "sw_preset",
    "nvenc_preset",
    "nvenc_vbv_increase",
    "nvenc_spatial_aq",
    "nvenc_h264_cavlc",
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum FieldValue {
    Boolean(bool),
    Integer(i64),
    Text(String),
}

impl FieldValue {
    pub fn sunshine_text(&self) -> String {
        match self {
            Self::Boolean(value) => value.to_string(),
            Self::Integer(value) => value.to_string(),
            Self::Text(value) => value.clone(),
        }
    }
}

pub fn validate_field(key: &str, value: &FieldValue) -> Result<(), InvalidTask> {
    let valid = match (key, value) {
        ("sunshine_name", FieldValue::Text(value)) => {
            !value.trim().is_empty()
                && value == value.trim()
                && value.chars().count() <= 32
                && !value
                    .chars()
                    .any(|c| c.is_control() || matches!(c, '#' | '=' | '[' | ']'))
        }
        ("min_log_level", FieldValue::Text(value)) => {
            matches!(value.as_str(), "info" | "warning" | "error" | "fatal")
        }
        ("sw_preset", FieldValue::Text(value)) => matches!(
            value.as_str(),
            "ultrafast"
                | "superfast"
                | "veryfast"
                | "faster"
                | "fast"
                | "medium"
                | "slow"
                | "slower"
                | "veryslow"
        ),
        ("qp", FieldValue::Integer(value)) => (0..=51).contains(value),
        ("hevc_mode" | "av1_mode", FieldValue::Integer(value)) => (0..=3).contains(value),
        ("min_threads", FieldValue::Integer(value)) => (1..=64).contains(value),
        ("nvenc_preset", FieldValue::Integer(value)) => (1..=7).contains(value),
        ("nvenc_vbv_increase", FieldValue::Integer(value)) => (0..=400).contains(value),
        ("nvenc_spatial_aq" | "nvenc_h264_cavlc", FieldValue::Boolean(_)) => true,
        _ => false,
    };
    if valid { Ok(()) } else { Err(InvalidTask) }
}
