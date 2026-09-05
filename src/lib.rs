//! Sunshine Manager is an independent service for managing Sunshine hosts.
//!
//! Product state and Sunshine credentials remain local; shared platform
//! metadata and administrator policy are supplied by sarmg-foundation-server.

pub mod client;
pub mod config;
pub mod cover_policy;
pub mod cover_proxy;
pub mod crypto;
pub mod database_schema;
pub mod db;
pub mod error;
pub mod http;
pub mod model;
pub mod operations;
pub mod release_bundle;
pub mod release_contract;
pub mod runtime_lock;

pub use config::ServeConfig;
pub use error::{AppError, AppResult};
