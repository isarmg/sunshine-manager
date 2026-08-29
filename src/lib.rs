//! Sunshine Manager is an independent service for managing Sunshine hosts.
//!
//! It owns its PostgreSQL schema, local administrator sessions and upstream
//! Sunshine credentials.

pub mod auth;
pub mod application;
pub mod cli;
pub mod client;
pub mod config;
pub mod crypto;
pub mod db;
pub mod domain;
pub mod error;
pub mod http;
pub mod infrastructure;
pub mod model;
pub mod operations;

pub use auth::{InternalAuth, InternalIdentity};
pub use config::ServeConfig;
pub use error::{AppError, AppResult};
