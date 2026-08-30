//! Sunshine Manager is an independent service for managing Sunshine hosts.
//!
//! It owns its SQLite schema, local administrator sessions and upstream
//! Sunshine credentials.

pub mod auth;
pub mod client;
pub mod config;
pub mod cover_policy;
pub mod crypto;
pub mod database_schema;
pub mod db;
pub mod error;
pub mod http;
pub mod login_admission;
pub mod model;
pub mod operations;
pub mod release_contract;
pub mod runtime_lock;

pub use auth::{InternalAuth, InternalIdentity};
pub use config::ServeConfig;
pub use error::{AppError, AppResult};
