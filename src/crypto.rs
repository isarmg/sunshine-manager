//! Product-owned authenticated encryption for Sunshine credentials and
//! durable remote-operation requests.

use aes_gcm::{
    Aes256Gcm, Key, Nonce,
    aead::{Aead, AeadCore, KeyInit, OsRng, Payload},
};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use hkdf::Hkdf;
use hmac::{Hmac, Mac};
use sha2::Sha256;
use subtle::ConstantTimeEq;

use crate::error::{AppError, AppResult};

const PREFIX: &str = "sunshine:v1:";
const AAD_FORMAT: &[u8] = b"sunshine-manager:aes-256-gcm:aad:v1";
const HOST_CREDENTIAL_DOMAIN: &[u8] = b"host-credential";
const HOST_SECRET_FIELD: &[u8] = b"secret";
const OPERATION_REQUEST_DOMAIN: &[u8] = b"operation-request";
const OPERATION_REQUEST_FIELD: &[u8] = b"request_ciphertext";
const HKDF_SALT: &[u8] = b"sunshine-manager:credential-master-key:hkdf-sha256:v1";
const REQUEST_FINGERPRINT_INFO: &[u8] =
    b"sunshine-manager:operation-request-fingerprint:hmac-sha256:v1";
const IDEMPOTENCY_KEY_HASH_INFO: &[u8] =
    b"sunshine-manager:operation-idempotency-key-hash:hmac-sha256:v1";

#[derive(Clone)]
pub struct SecretBox {
    current_id: String,
    current: [u8; 32],
    request_fingerprint_key: [u8; 32],
    idempotency_key_hash_key: [u8; 32],
}

impl SecretBox {
    pub fn new(current_id: impl Into<String>, current: [u8; 32]) -> anyhow::Result<Self> {
        let current_id = validate_key_id(current_id.into())?;
        let derivation = Hkdf::<Sha256>::new(Some(HKDF_SALT), &current);
        let request_fingerprint_key =
            derive_key(&derivation, REQUEST_FINGERPRINT_INFO, "request fingerprint")?;
        let idempotency_key_hash_key = derive_key(
            &derivation,
            IDEMPOTENCY_KEY_HASH_INFO,
            "idempotency key hash",
        )?;
        Ok(Self {
            current_id,
            current,
            request_fingerprint_key,
            idempotency_key_hash_key,
        })
    }

    /// Seal one Host's Sunshine Basic Auth password. The authenticated context
    /// prevents a valid ciphertext from being moved to another Host or field.
    pub fn encrypt_host_credential(&self, host_id: &str, value: &str) -> AppResult<String> {
        self.encrypt_with_aad(value, &host_credential_aad(host_id))
    }

    pub fn decrypt_host_credential(&self, host_id: &str, value: &str) -> AppResult<String> {
        self.decrypt_with_aad(value, &host_credential_aad(host_id))
    }

    /// Seal one durable operation request. Both the row identity and declared
    /// action are authenticated so neither cross-row nor cross-action swaps are
    /// accepted before the strict request enum is parsed.
    pub fn encrypt_operation_request(
        &self,
        operation_id: &str,
        action: &str,
        value: &str,
    ) -> AppResult<String> {
        self.encrypt_with_aad(value, &operation_request_aad(operation_id, action))
    }

    pub fn decrypt_operation_request(
        &self,
        operation_id: &str,
        action: &str,
        value: &str,
    ) -> AppResult<String> {
        self.decrypt_with_aad(value, &operation_request_aad(operation_id, action))
    }

    fn encrypt_with_aad(&self, value: &str, aad: &[u8]) -> AppResult<String> {
        let payload = seal(&self.current, value.as_bytes(), aad)?;
        Ok(format!(
            "{PREFIX}{}:{}",
            self.current_id,
            STANDARD.encode(payload)
        ))
    }

    fn decrypt_with_aad(&self, value: &str, aad: &[u8]) -> AppResult<String> {
        let rest = value.strip_prefix(PREFIX).ok_or(AppError::Crypto)?;
        let (id, payload) = rest.split_once(':').ok_or(AppError::Crypto)?;
        if id != self.current_id {
            return Err(AppError::Crypto);
        }
        let plaintext = open(&self.current, &decode_payload(payload)?, aad)?;
        String::from_utf8(plaintext).map_err(|_| AppError::Crypto)
    }

    /// Stable keyed fingerprint used only to decide whether a repeated
    /// Idempotency-Key carries the exact same canonical request JSON.
    pub fn operation_request_fingerprint(&self, canonical_request: &str) -> [u8; 32] {
        hmac_sha256(&self.request_fingerprint_key, canonical_request.as_bytes())
    }

    /// Stable, non-reversible database lookup key for a caller-provided
    /// Idempotency-Key. Its HKDF info is deliberately distinct from request
    /// fingerprints, so equal input bytes cannot cross protocol domains.
    pub fn operation_idempotency_key_hash(&self, idempotency_key: &str) -> [u8; 32] {
        hmac_sha256(&self.idempotency_key_hash_key, idempotency_key.as_bytes())
    }
}

pub(crate) fn constant_time_equal_32(left: &[u8], right: &[u8; 32]) -> bool {
    let Ok(left) = <&[u8; 32]>::try_from(left) else {
        return false;
    };
    bool::from(left.as_slice().ct_eq(right.as_slice()))
}

fn derive_key(derivation: &Hkdf<Sha256>, info: &[u8], purpose: &str) -> anyhow::Result<[u8; 32]> {
    let mut derived = [0_u8; 32];
    derivation
        .expand(info, &mut derived)
        .map_err(|_| anyhow::anyhow!("HKDF output length is invalid for {purpose}"))?;
    Ok(derived)
}

fn hmac_sha256(key: &[u8; 32], value: &[u8]) -> [u8; 32] {
    let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(key)
        .expect("an HMAC-SHA-256 key accepts the derived 32-byte key");
    mac.update(value);
    mac.finalize().into_bytes().into()
}

fn seal(key: &[u8; 32], plaintext: &[u8], aad: &[u8]) -> AppResult<Vec<u8>> {
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key));
    let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
    let mut payload = nonce.to_vec();
    payload.extend(
        cipher
            .encrypt(
                &nonce,
                Payload {
                    msg: plaintext,
                    aad,
                },
            )
            .map_err(|_| AppError::Crypto)?,
    );
    Ok(payload)
}

fn open(key: &[u8; 32], payload: &[u8], aad: &[u8]) -> AppResult<Vec<u8>> {
    if payload.len() <= 12 {
        return Err(AppError::Crypto);
    }
    let (nonce, ciphertext) = payload.split_at(12);
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key));
    cipher
        .decrypt(
            Nonce::from_slice(nonce),
            Payload {
                msg: ciphertext,
                aad,
            },
        )
        .map_err(|_| AppError::Crypto)
}

/// Length framing makes the deterministic context injective even if a future
/// identifier or action is allowed to contain a separator used by another
/// component. The bytes are deliberately independent of serde or display
/// formatting because they are part of the current persistent crypto contract.
fn authenticated_context(domain: &[u8], components: &[&[u8]]) -> Vec<u8> {
    let mut context = Vec::with_capacity(
        AAD_FORMAT.len()
            + domain.len()
            + components
                .iter()
                .map(|value| value.len() + 8)
                .sum::<usize>()
            + 16,
    );
    append_context_component(&mut context, AAD_FORMAT);
    append_context_component(&mut context, domain);
    for component in components {
        append_context_component(&mut context, component);
    }
    context
}

fn append_context_component(context: &mut Vec<u8>, value: &[u8]) {
    let length = u64::try_from(value.len()).expect("AAD component length must fit u64");
    context.extend_from_slice(&length.to_be_bytes());
    context.extend_from_slice(value);
}

fn host_credential_aad(host_id: &str) -> Vec<u8> {
    authenticated_context(
        HOST_CREDENTIAL_DOMAIN,
        &[host_id.as_bytes(), HOST_SECRET_FIELD],
    )
}

fn operation_request_aad(operation_id: &str, action: &str) -> Vec<u8> {
    authenticated_context(
        OPERATION_REQUEST_DOMAIN,
        &[
            operation_id.as_bytes(),
            action.as_bytes(),
            OPERATION_REQUEST_FIELD,
        ],
    )
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
    use sha2::Digest as _;

    #[test]
    fn host_ciphertext_round_trips_and_is_randomized() {
        let secrets = SecretBox::new("primary", [3; 32]).unwrap();
        let first = secrets
            .encrypt_host_credential("host-a", "password")
            .unwrap();
        let second = secrets
            .encrypt_host_credential("host-a", "password")
            .unwrap();
        assert_ne!(first, second);
        assert_eq!(
            secrets.decrypt_host_credential("host-a", &first).unwrap(),
            "password"
        );
        assert!(!first.contains("password"));

        let rest = first.strip_prefix(PREFIX).unwrap();
        let (key_id, encoded) = rest.split_once(':').unwrap();
        let mut tampered_payload = decode_payload(encoded).unwrap();
        *tampered_payload.last_mut().unwrap() ^= 1;
        let tampered = format!("{PREFIX}{key_id}:{}", STANDARD.encode(tampered_payload));
        assert!(
            secrets
                .decrypt_host_credential("host-a", &tampered)
                .is_err()
        );
    }

    #[test]
    fn authenticated_context_rejects_record_action_and_domain_swaps() {
        let secrets = SecretBox::new("primary", [3; 32]).unwrap();
        let host_ciphertext = secrets
            .encrypt_host_credential("host-a", "password")
            .unwrap();
        assert!(
            secrets
                .decrypt_host_credential("host-b", &host_ciphertext)
                .is_err()
        );
        assert!(
            secrets
                .decrypt_operation_request("host-a", "secret", &host_ciphertext,)
                .is_err()
        );

        let operation_ciphertext = secrets
            .encrypt_operation_request("op-a", "sunshine.pin", r#"{"pin":"1234"}"#)
            .unwrap();
        assert_eq!(
            secrets
                .decrypt_operation_request("op-a", "sunshine.pin", &operation_ciphertext)
                .unwrap(),
            r#"{"pin":"1234"}"#
        );
        assert!(
            secrets
                .decrypt_operation_request("op-b", "sunshine.pin", &operation_ciphertext)
                .is_err()
        );
        assert!(
            secrets
                .decrypt_operation_request("op-a", "sunshine.config.save", &operation_ciphertext)
                .is_err()
        );
    }

    #[test]
    fn ciphertext_for_another_key_id_or_empty_aad_is_rejected() {
        let source = SecretBox::new("source", [3; 32]).unwrap();
        let active = SecretBox::new("active", [4; 32]).unwrap();
        let ciphertext = source
            .encrypt_host_credential("host-a", "password")
            .unwrap();

        assert!(
            active
                .decrypt_host_credential("host-a", &ciphertext)
                .is_err()
        );

        // This constructs the pre-AAD form only to prove that the current
        // reader has no empty-AAD fallback or legacy compatibility branch.
        let legacy_payload = seal(&[3; 32], b"password", b"").unwrap();
        let legacy = format!("{PREFIX}source:{}", STANDARD.encode(legacy_payload));
        assert!(source.decrypt_host_credential("host-a", &legacy).is_err());
    }

    #[test]
    fn operation_hmacs_are_stable_domain_separated_and_key_bound() {
        let first = SecretBox::new("primary", [21; 32]).unwrap();
        let same_master = SecretBox::new("primary", [21; 32]).unwrap();
        let other_master = SecretBox::new("primary", [22; 32]).unwrap();
        let low_entropy = r#"{"kind":"pin","pin":"1234","name":"laptop"}"#;

        let fingerprint = first.operation_request_fingerprint(low_entropy);
        assert_eq!(
            fingerprint,
            first.operation_request_fingerprint(low_entropy)
        );
        assert_eq!(
            fingerprint,
            same_master.operation_request_fingerprint(low_entropy)
        );
        assert_ne!(
            fingerprint,
            other_master.operation_request_fingerprint(low_entropy)
        );
        let idempotency_hash = first.operation_idempotency_key_hash(low_entropy);
        assert_eq!(
            idempotency_hash,
            first.operation_idempotency_key_hash(low_entropy)
        );
        assert_eq!(
            idempotency_hash,
            same_master.operation_idempotency_key_hash(low_entropy)
        );
        assert_ne!(
            idempotency_hash,
            other_master.operation_idempotency_key_hash(low_entropy)
        );
        assert_ne!(
            fingerprint, idempotency_hash,
            "separate HKDF info values must keep the HMAC domains independent"
        );

        let bare_sha256: [u8; 32] = Sha256::digest(low_entropy.as_bytes()).into();
        assert_ne!(fingerprint, bare_sha256);
        assert!(constant_time_equal_32(&fingerprint, &fingerprint));
        let mut different = fingerprint;
        different[31] ^= 1;
        assert!(!constant_time_equal_32(&fingerprint, &different));
        assert!(!constant_time_equal_32(&fingerprint[..31], &fingerprint));
    }
}
