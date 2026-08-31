use axum::{
    Json,
    http::{HeaderValue, StatusCode, header::CACHE_CONTROL},
    response::{IntoResponse, Response},
};
use sarmg_error::{ErrorCode, ErrorEnvelope};
use thiserror::Error;

pub type AppResult<T> = Result<T, AppError>;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("{0}")]
    BadRequest(String),
    #[error("unauthorized")]
    Unauthorized,
    #[error("{0}")]
    Forbidden(String),
    #[error("{0}")]
    NotFound(String),
    #[error("{0}")]
    Conflict(String),
    #[error("too many requests; retry after {retry_after} seconds")]
    TooManyRequests { retry_after: u64 },
    #[error("{0}")]
    Upstream(String),
    #[error("database unavailable")]
    Database(#[source] sqlx::Error),
    #[error("cryptographic operation failed")]
    Crypto,
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

impl AppError {
    fn status(&self) -> StatusCode {
        match self {
            Self::BadRequest(_) => StatusCode::BAD_REQUEST,
            Self::Unauthorized => StatusCode::UNAUTHORIZED,
            Self::Forbidden(_) => StatusCode::FORBIDDEN,
            Self::NotFound(_) => StatusCode::NOT_FOUND,
            Self::Conflict(_) => StatusCode::CONFLICT,
            Self::TooManyRequests { .. } => StatusCode::TOO_MANY_REQUESTS,
            Self::Upstream(_) => StatusCode::BAD_GATEWAY,
            Self::Database(_) => StatusCode::SERVICE_UNAVAILABLE,
            Self::Crypto | Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    fn code(&self) -> &'static str {
        match self {
            Self::BadRequest(_) => "bad_request",
            Self::Unauthorized => "unauthorized",
            Self::Forbidden(_) => "forbidden",
            Self::NotFound(_) => "not_found",
            Self::Conflict(_) => "conflict",
            Self::TooManyRequests { .. } => "too_many_requests",
            Self::Upstream(_) => "upstream_error",
            Self::Database(_) => "database_unavailable",
            Self::Crypto => "crypto_error",
            Self::Internal(_) => "internal_error",
        }
    }

    fn retryable(&self) -> bool {
        matches!(
            self,
            Self::TooManyRequests { .. } | Self::Upstream(_) | Self::Database(_)
        )
    }
}

impl From<sqlx::Error> for AppError {
    fn from(value: sqlx::Error) -> Self {
        Self::Database(value)
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status = self.status();
        let code =
            ErrorCode::new(self.code()).expect("AppError codes are valid current identifiers");
        let retryable = self.retryable();
        let retry_after = match &self {
            Self::TooManyRequests { retry_after } => Some(*retry_after),
            _ => None,
        };
        let message = match &self {
            Self::BadRequest(message)
            | Self::Forbidden(message)
            | Self::NotFound(message)
            | Self::Conflict(message) => message.clone(),
            // Upstream diagnostics can contain remote URLs, response snippets
            // or transport details. They remain available to the protected
            // server log through Display but never cross the API boundary.
            Self::Upstream(_) => "Sunshine upstream request failed".to_string(),
            Self::TooManyRequests { retry_after } => {
                format!("too many login attempts; retry after {retry_after} seconds")
            }
            Self::Unauthorized => "unauthorized".to_string(),
            Self::Database(_) => "database unavailable".to_string(),
            Self::Crypto | Self::Internal(_) => "internal error".to_string(),
        };
        if status.is_server_error() {
            tracing::error!(error = %self, "Sunshine worker request failed");
        }
        let envelope = ErrorEnvelope::with_code(code, message).retryable(retryable);
        let mut response = (status, Json(envelope)).into_response();
        if let Some(retry_after) = retry_after
            && let Ok(value) = retry_after.to_string().parse()
        {
            response
                .headers_mut()
                .insert(axum::http::header::RETRY_AFTER, value);
        }
        response.headers_mut().insert(
            CACHE_CONTROL,
            HeaderValue::from_static("no-store, private, max-age=0"),
        );
        response
    }
}

#[cfg(test)]
mod tests {
    use http_body_util::BodyExt;
    use serde_json::json;

    use super::*;

    #[tokio::test]
    async fn rate_limit_response_has_strict_envelope_and_retry_after() {
        let response = AppError::TooManyRequests { retry_after: 17 }.into_response();
        assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(response.headers()[axum::http::header::RETRY_AFTER], "17");
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let envelope: ErrorEnvelope = serde_json::from_slice(&body).unwrap();
        assert_eq!(
            serde_json::to_value(envelope).unwrap(),
            json!({
                "code": "too_many_requests",
                "message": "too many login attempts; retry after 17 seconds",
                "retryable": true
            })
        );
    }

    #[tokio::test]
    async fn internal_diagnostics_are_not_exposed() {
        let response = AppError::Internal(anyhow::anyhow!("private diagnostic")).into_response();
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(
            value,
            json!({
                "code": "internal_error",
                "message": "internal error",
                "retryable": false
            })
        );
    }

    #[tokio::test]
    async fn upstream_diagnostics_are_not_exposed() {
        let response = AppError::Upstream("private remote body and transport diagnostic".into())
            .into_response();
        assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(
            value,
            json!({
                "code": "upstream_error",
                "message": "Sunshine upstream request failed",
                "retryable": true
            })
        );
    }
}
