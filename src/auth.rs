use std::time::Duration;

use crate::error::{AppError, AppResult};

const SECURE_SESSION_COOKIE: &str = "__Host-sunshine_session";
const DEVELOPMENT_SESSION_COOKIE: &str = "sunshine_session";

#[derive(Clone)]
pub struct InternalAuth {
    absolute_ttl: Duration,
    idle_ttl: Duration,
    secure_cookie: bool,
}

pub struct IssuedSession {
    pub session_id: String,
    pub token: String,
    pub token_hash: Vec<u8>,
    pub csrf_token: String,
    pub csrf_hash: Vec<u8>,
    pub idle_expires_at_micros: i64,
    pub absolute_expires_at_micros: i64,
}

impl InternalAuth {
    pub fn new(absolute_ttl: Duration, idle_ttl: Duration, secure_cookie: bool) -> AppResult<Self> {
        if absolute_ttl.is_zero() || idle_ttl.is_zero() || idle_ttl > absolute_ttl {
            return Err(AppError::BadRequest(
                "session TTLs must be non-zero and idle TTL cannot exceed absolute TTL".into(),
            ));
        }
        Ok(Self {
            absolute_ttl,
            idle_ttl,
            secure_cookie,
        })
    }

    pub fn issue(&self, now_micros: i64) -> AppResult<IssuedSession> {
        let token = sarmg_admin_auth::random_token()
            .map_err(|error| AppError::Internal(anyhow::anyhow!(error)))?;
        let csrf_token = sarmg_admin_auth::random_token()
            .map_err(|error| AppError::Internal(anyhow::anyhow!(error)))?;
        let absolute_expires_at_micros = add_duration(now_micros, self.absolute_ttl)?;
        let idle_expires_at_micros = add_duration(now_micros, self.idle_ttl)?;
        Ok(IssuedSession {
            session_id: uuid::Uuid::new_v4().to_string(),
            token_hash: token_hash(&token),
            csrf_hash: token_hash(&csrf_token),
            token,
            csrf_token,
            idle_expires_at_micros,
            absolute_expires_at_micros,
        })
    }

    pub fn refreshed_idle_expiry(
        &self,
        now_micros: i64,
        absolute_expires_at_micros: i64,
    ) -> AppResult<i64> {
        Ok(add_duration(now_micros, self.idle_ttl)?.min(absolute_expires_at_micros))
    }

    pub fn session_cookie(&self, token: &str) -> String {
        let mut value = format!(
            "{}={token}; Path=/; HttpOnly; SameSite=Strict; Max-Age={}",
            self.session_cookie_name(),
            self.absolute_ttl.as_secs()
        );
        if self.secure_cookie {
            value.push_str("; Secure");
        }
        value
    }

    pub fn expired_session_cookie(&self) -> String {
        expired_cookie(self.session_cookie_name(), true, self.secure_cookie)
    }

    pub fn parse_session_token(&self, headers: &axum::http::HeaderMap) -> Option<String> {
        parse_cookie_token(self.session_cookie_name(), headers)
    }

    pub fn origin_mode(&self) -> sarmg_admin_auth::AdministratorOriginMode {
        if self.secure_cookie {
            sarmg_admin_auth::AdministratorOriginMode::ProductionHttps
        } else {
            sarmg_admin_auth::AdministratorOriginMode::LoopbackDevelopmentHttp
        }
    }

    fn session_cookie_name(&self) -> &'static str {
        if self.secure_cookie {
            SECURE_SESSION_COOKIE
        } else {
            DEVELOPMENT_SESSION_COOKIE
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InternalIdentity {
    pub subject: String,
    pub username: String,
    pub session_id: String,
    pub csrf_hash: Vec<u8>,
}

pub fn hash_password(password: &str) -> AppResult<String> {
    sarmg_admin_auth::hash_password(password).map_err(|error| match error {
        sarmg_admin_auth::Error::InvalidPassword => AppError::BadRequest(error.to_string()),
        _ => AppError::Internal(anyhow::anyhow!(error)),
    })
}

pub fn verify_password(password: &str, encoded: &str) -> bool {
    sarmg_admin_auth::verify_password(password, encoded)
}

pub fn normalize_administrator_username(username: &str) -> AppResult<String> {
    sarmg_admin_auth::normalize_administrator_username(username)
        .map_err(|error| AppError::BadRequest(error.to_string()))
}

pub fn token_hash(token: &str) -> Vec<u8> {
    sarmg_admin_auth::token_hash(token).to_vec()
}

pub fn token_matches_hash(token: &str, expected: &[u8]) -> bool {
    sarmg_admin_auth::token_matches_hash(token, expected)
}

fn add_duration(now_micros: i64, duration: Duration) -> AppResult<i64> {
    let duration_micros = i64::try_from(duration.as_micros())
        .map_err(|_| AppError::Internal(anyhow::anyhow!("session TTL is too large")))?;
    now_micros
        .checked_add(duration_micros)
        .ok_or_else(|| AppError::Internal(anyhow::anyhow!("session expiry overflow")))
}

fn parse_cookie_token(name: &str, headers: &axum::http::HeaderMap) -> Option<String> {
    let mut values = headers.get_all(axum::http::header::COOKIE).iter();
    let header = values.next()?.to_str().ok()?;
    if values.next().is_some() {
        return None;
    }
    sarmg_admin_auth::parse_cookie_value(header, name)
        .filter(|token| sarmg_admin_auth::is_token_shape(token))
        .map(str::to_owned)
}

fn expired_cookie(name: &str, http_only: bool, secure: bool) -> String {
    let mut value = format!("{name}=; Path=/; SameSite=Strict; Max-Age=0");
    if http_only {
        value.push_str("; HttpOnly");
    }
    if secure {
        value.push_str("; Secure");
    }
    value
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn issued_tokens_are_random_hashed_and_use_scoped_cookie_names() {
        let secure =
            InternalAuth::new(Duration::from_secs(3_600), Duration::from_secs(600), true).unwrap();
        let first = secure.issue(1_000_000).unwrap();
        let second = secure.issue(1_000_000).unwrap();
        assert_ne!(first.token, second.token);
        assert_ne!(first.csrf_token, second.csrf_token);
        assert_eq!(first.token_hash.len(), 32);
        assert_eq!(first.csrf_hash.len(), 32);
        assert!(token_matches_hash(&first.token, &first.token_hash));
        assert!(token_matches_hash(&first.csrf_token, &first.csrf_hash));
        assert!(
            secure
                .session_cookie(&first.token)
                .starts_with("__Host-sunshine_session=")
        );
        assert!(secure.session_cookie(&first.token).contains("; Secure"));
        let mut duplicate_headers = axum::http::HeaderMap::new();
        duplicate_headers.append(
            axum::http::header::COOKIE,
            format!("{}={}", secure.session_cookie_name(), first.token)
                .parse()
                .unwrap(),
        );
        duplicate_headers.append(
            axum::http::header::COOKIE,
            format!("{}={}", secure.session_cookie_name(), second.token)
                .parse()
                .unwrap(),
        );
        assert!(secure.parse_session_token(&duplicate_headers).is_none());

        let mut duplicate_pair = axum::http::HeaderMap::new();
        duplicate_pair.insert(
            axum::http::header::COOKIE,
            format!(
                "{}={}; {}={}",
                secure.session_cookie_name(),
                first.token,
                secure.session_cookie_name(),
                second.token
            )
            .parse()
            .unwrap(),
        );
        assert!(secure.parse_session_token(&duplicate_pair).is_none());
    }

    #[test]
    fn passwords_use_only_the_exact_current_argon2id_policy() {
        let password = "current-sunshine-password";
        let encoded = hash_password(password).unwrap();
        sarmg_admin_auth::require_current_password_hash(&encoded).unwrap();
        assert!(verify_password(password, &encoded));
        assert!(!verify_password("wrong-current-password", &encoded));
        let reordered = encoded.replace("m=19456,t=2,p=1", "m=19456,p=1,t=2");
        assert!(!verify_password(password, &reordered));

        assert!(!verify_password(password, "not-a-password-hash"));
    }

    #[test]
    fn password_length_is_a_bounded_current_contract() {
        assert!(hash_password(&"a".repeat(sarmg_admin_auth::PASSWORD_MIN_BYTES - 1)).is_err());
        assert!(hash_password(&"a".repeat(sarmg_admin_auth::PASSWORD_MAX_BYTES + 1)).is_err());
        assert!(!verify_password(
            &"a".repeat(sarmg_admin_auth::PASSWORD_MAX_BYTES + 1),
            "not-a-hash"
        ));
    }
}
