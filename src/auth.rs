use std::time::Duration;

use crate::error::{AppError, AppResult};

pub const SESSION_COOKIE: &str = "sunshine_session";

#[derive(Clone)]
pub struct InternalAuth {
    issuer: isarmg_auth::SessionIssuer,
}

impl InternalAuth {
    pub fn new(secret: Vec<u8>, ttl: Duration, cookie_secure: bool) -> AppResult<Self> {
        Ok(Self {
            issuer: isarmg_auth::SessionIssuer::new(secret, ttl, cookie_secure)
                .map_err(|error| AppError::Internal(anyhow::anyhow!(error)))?,
        })
    }

    pub fn issue_session(&self, subject: &str) -> AppResult<String> {
        self.issuer
            .issue(subject)
            .map_err(|error| AppError::Internal(anyhow::anyhow!(error)))
    }

    pub fn verify_session(&self, token: &str) -> AppResult<String> {
        self.issuer
            .verify(token)
            .map_err(|_| AppError::Unauthorized)
    }

    pub fn session_cookie(&self, token: &str) -> String {
        self.issuer.session_cookie(SESSION_COOKIE, token)
    }

    pub fn expired_session_cookie(&self) -> String {
        self.issuer.expired_cookie(SESSION_COOKIE)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InternalIdentity {
    pub subject: String,
}

pub use isarmg_auth::{hash_password, verify_password};

pub fn parse_cookie_token(headers: &axum::http::HeaderMap) -> Option<String> {
    isarmg_auth::parse_cookie_token(SESSION_COOKIE, headers)
}
