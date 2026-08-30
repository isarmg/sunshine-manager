use std::time::Duration;

use argon2::{
    Algorithm, Argon2, Params, Version,
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, Salt, SaltString},
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use rand::{RngCore, rngs::OsRng};
use sha2::{Digest, Sha256};

use crate::error::{AppError, AppResult};

const SECURE_SESSION_COOKIE: &str = "__Host-sunshine_session";
const DEVELOPMENT_SESSION_COOKIE: &str = "sunshine_session";
const SECURE_CSRF_COOKIE: &str = "__Host-sunshine_csrf";
const DEVELOPMENT_CSRF_COOKIE: &str = "sunshine_csrf";
const PASSWORD_MIN_BYTES: usize = 12;
const PASSWORD_MAX_BYTES: usize = 1024;
const ARGON2_MEMORY_KIB: u32 = 19_456;
const ARGON2_ITERATIONS: u32 = 2;
const ARGON2_LANES: u32 = 1;
const ARGON2_OUTPUT_BYTES: usize = 32;

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
        let token = random_token();
        let csrf_token = random_token();
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

    pub fn csrf_cookie(&self, token: &str) -> String {
        let mut value = format!(
            "{}={token}; Path=/; SameSite=Strict; Max-Age={}",
            self.csrf_cookie_name(),
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

    pub fn expired_csrf_cookie(&self) -> String {
        expired_cookie(self.csrf_cookie_name(), false, self.secure_cookie)
    }

    pub fn parse_session_token(&self, headers: &axum::http::HeaderMap) -> Option<String> {
        parse_cookie_token(self.session_cookie_name(), headers)
    }

    pub fn parse_csrf_token(&self, headers: &axum::http::HeaderMap) -> Option<String> {
        parse_cookie_token(self.csrf_cookie_name(), headers)
    }

    fn session_cookie_name(&self) -> &'static str {
        if self.secure_cookie {
            SECURE_SESSION_COOKIE
        } else {
            DEVELOPMENT_SESSION_COOKIE
        }
    }

    fn csrf_cookie_name(&self) -> &'static str {
        if self.secure_cookie {
            SECURE_CSRF_COOKIE
        } else {
            DEVELOPMENT_CSRF_COOKIE
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InternalIdentity {
    pub subject: String,
    pub email: String,
    pub session_id: String,
    pub csrf_hash: Vec<u8>,
}

pub fn hash_password(password: &str) -> AppResult<String> {
    validate_password(password)?;
    let salt = SaltString::generate(&mut argon2::password_hash::rand_core::OsRng);
    current_argon2()
        .hash_password(password.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(|error| AppError::Internal(anyhow::anyhow!(error)))
}

pub fn verify_password(password: &str, encoded: &str) -> bool {
    if validate_password(password).is_err() {
        return false;
    }
    let Ok(hash) = PasswordHash::new(encoded) else {
        return false;
    };
    if hash.to_string() != encoded || !password_hash_uses_current_policy(&hash) {
        return false;
    }
    current_argon2()
        .verify_password(password.as_bytes(), &hash)
        .is_ok()
}

fn validate_password(password: &str) -> AppResult<()> {
    if !(PASSWORD_MIN_BYTES..=PASSWORD_MAX_BYTES).contains(&password.len()) {
        return Err(AppError::BadRequest(format!(
            "password must contain between {PASSWORD_MIN_BYTES} and {PASSWORD_MAX_BYTES} bytes"
        )));
    }
    Ok(())
}

fn current_argon2() -> Argon2<'static> {
    let params = Params::new(
        ARGON2_MEMORY_KIB,
        ARGON2_ITERATIONS,
        ARGON2_LANES,
        Some(ARGON2_OUTPUT_BYTES),
    )
    .expect("compiled Argon2id policy is valid");
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
}

fn password_hash_uses_current_policy(hash: &PasswordHash<'_>) -> bool {
    let mut salt = [0_u8; Salt::MAX_LENGTH];
    hash.algorithm.as_str() == "argon2id"
        && hash.version == Some(Version::V0x13.into())
        && hash.params.as_str() == "m=19456,t=2,p=1"
        && hash.params.get_decimal("m") == Some(ARGON2_MEMORY_KIB)
        && hash.params.get_decimal("t") == Some(ARGON2_ITERATIONS)
        && hash.params.get_decimal("p") == Some(ARGON2_LANES)
        && hash
            .salt
            .and_then(|value| value.decode_b64(&mut salt).ok())
            .is_some_and(|decoded| decoded.len() == Salt::RECOMMENDED_LENGTH)
        && hash
            .hash
            .as_ref()
            .is_some_and(|output| output.len() == ARGON2_OUTPUT_BYTES)
}

pub fn token_hash(token: &str) -> Vec<u8> {
    Sha256::digest(token.as_bytes()).to_vec()
}

pub fn token_matches_hash(token: &str, expected: &[u8]) -> bool {
    constant_time_eq(&token_hash(token), expected)
}

fn random_token() -> String {
    let mut bytes = [0_u8; 32];
    OsRng.fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

fn add_duration(now_micros: i64, duration: Duration) -> AppResult<i64> {
    let duration_micros = i64::try_from(duration.as_micros())
        .map_err(|_| AppError::Internal(anyhow::anyhow!("session TTL is too large")))?;
    now_micros
        .checked_add(duration_micros)
        .ok_or_else(|| AppError::Internal(anyhow::anyhow!("session expiry overflow")))
}

fn parse_cookie_token(name: &str, headers: &axum::http::HeaderMap) -> Option<String> {
    headers
        .get(axum::http::header::COOKIE)?
        .to_str()
        .ok()?
        .split(';')
        .find_map(|part| {
            let (key, value) = part.trim().split_once('=')?;
            (key == name && !value.is_empty()).then(|| value.to_string())
        })
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

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    left.iter()
        .zip(right)
        .fold(0_u8, |difference, (left, right)| {
            difference | (left ^ right)
        })
        == 0
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
        assert!(!secure.csrf_cookie(&first.csrf_token).contains("HttpOnly"));
    }

    #[test]
    fn passwords_use_only_the_exact_current_argon2id_policy() {
        let password = "current-sunshine-password";
        let encoded = hash_password(password).unwrap();
        let parsed = PasswordHash::new(&encoded).unwrap();
        assert!(password_hash_uses_current_policy(&parsed));
        assert!(verify_password(password, &encoded));
        assert!(!verify_password("wrong-current-password", &encoded));
        let reordered = encoded.replace("m=19456,t=2,p=1", "m=19456,p=1,t=2");
        assert!(!verify_password(password, &reordered));

        let weaker = Argon2::new(
            Algorithm::Argon2id,
            Version::V0x13,
            Params::new(8_192, 1, 1, Some(ARGON2_OUTPUT_BYTES)).unwrap(),
        )
        .hash_password(
            password.as_bytes(),
            &SaltString::generate(&mut argon2::password_hash::rand_core::OsRng),
        )
        .unwrap()
        .to_string();
        assert!(!verify_password(password, &weaker));

        let other_algorithm = Argon2::new(
            Algorithm::Argon2i,
            Version::V0x13,
            Params::new(
                ARGON2_MEMORY_KIB,
                ARGON2_ITERATIONS,
                ARGON2_LANES,
                Some(ARGON2_OUTPUT_BYTES),
            )
            .unwrap(),
        )
        .hash_password(
            password.as_bytes(),
            &SaltString::generate(&mut argon2::password_hash::rand_core::OsRng),
        )
        .unwrap()
        .to_string();
        assert!(!verify_password(password, &other_algorithm));
    }

    #[test]
    fn password_length_is_a_bounded_current_contract() {
        assert!(hash_password(&"a".repeat(PASSWORD_MIN_BYTES - 1)).is_err());
        assert!(hash_password(&"a".repeat(PASSWORD_MAX_BYTES + 1)).is_err());
        assert!(!verify_password(
            &"a".repeat(PASSWORD_MAX_BYTES + 1),
            "not-a-hash"
        ));
    }
}
