//! Product-owned encryption for Sunshine upstream passwords.

use aes_gcm::{
    Aes256Gcm, Key, Nonce,
    aead::{Aead, AeadCore, KeyInit, OsRng},
};
use base64::{Engine as _, engine::general_purpose::STANDARD};

use crate::error::{AppError, AppResult};

const PREFIX: &str = "sunshine:v1:";

#[derive(Clone)]
pub struct SecretBox {
    current_id: String,
    current: [u8; 32],
}

impl SecretBox {
    pub fn new(current_id: impl Into<String>, current: [u8; 32]) -> anyhow::Result<Self> {
        let current_id = validate_key_id(current_id.into())?;
        Ok(Self {
            current_id,
            current,
        })
    }

    pub fn encrypt(&self, value: &str) -> AppResult<String> {
        let payload = seal(&self.current, value.as_bytes())?;
        Ok(format!(
            "{PREFIX}{}:{}",
            self.current_id,
            STANDARD.encode(payload)
        ))
    }

    pub fn decrypt(&self, value: &str) -> AppResult<String> {
        let rest = value.strip_prefix(PREFIX).ok_or(AppError::Crypto)?;
        let (id, payload) = rest.split_once(':').ok_or(AppError::Crypto)?;
        if id != self.current_id {
            return Err(AppError::Crypto);
        }
        let plaintext = open(&self.current, &decode_payload(payload)?)?;
        String::from_utf8(plaintext).map_err(|_| AppError::Crypto)
    }

    pub fn hmac_key(&self) -> &[u8; 32] {
        &self.current
    }
}

fn seal(key: &[u8; 32], plaintext: &[u8]) -> AppResult<Vec<u8>> {
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key));
    let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
    let mut payload = nonce.to_vec();
    payload.extend(
        cipher
            .encrypt(&nonce, plaintext)
            .map_err(|_| AppError::Crypto)?,
    );
    Ok(payload)
}

fn open(key: &[u8; 32], payload: &[u8]) -> AppResult<Vec<u8>> {
    if payload.len() <= 12 {
        return Err(AppError::Crypto);
    }
    let (nonce, ciphertext) = payload.split_at(12);
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key));
    cipher
        .decrypt(Nonce::from_slice(nonce), ciphertext)
        .map_err(|_| AppError::Crypto)
}

fn decode_payload(value: &str) -> AppResult<Vec<u8>> {
    STANDARD.decode(value).map_err(|_| AppError::Crypto)
}

fn validate_key_id(value: String) -> anyhow::Result<String> {
    anyhow::ensure!(
        !value.is_empty()
            && value.len() <= 64
            && value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_')),
        "key id must contain 1-64 ASCII letters, digits, '-' or '_'"
    );
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn module_ciphertext_round_trips_and_is_randomized() {
        let secrets = SecretBox::new("primary", [3; 32]).unwrap();
        let first = secrets.encrypt("password").unwrap();
        let second = secrets.encrypt("password").unwrap();
        assert_ne!(first, second);
        assert_eq!(secrets.decrypt(&first).unwrap(), "password");
        assert!(!first.contains("password"));
    }

    #[test]
    fn ciphertext_for_another_key_id_is_not_a_runtime_compatibility_path() {
        let old = SecretBox::new("old", [3; 32]).unwrap();
        let current = SecretBox::new("current", [4; 32]).unwrap();
        let ciphertext = old.encrypt("password").unwrap();

        assert!(current.decrypt(&ciphertext).is_err());
    }
}
