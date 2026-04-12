//! AES-256-GCM field-level encryption for sensitive database columns.
//!
//! # Storage format
//!
//! Encrypted values are stored as a UTF-8 string:
//!
//! ```text
//! enc:<base64url-no-padding(12-byte-nonce || ciphertext || 16-byte-gcm-tag)>
//! ```
//!
//! The `enc:` prefix is a sentinel that lets read paths detect whether a
//! column holds ciphertext or legacy plaintext.  Legacy plaintext is
//! returned as-is (passthrough), enabling a forward-only migration without
//! touching existing rows.
//!
//! # Key management
//!
//! The 32-byte key is loaded from the `FIELD_ENCRYPTION_KEY` environment
//! variable (base64url, no padding).  A default insecure key is used when
//! the variable is absent — this is intentional for local development.
//! Production deployments **must** set a randomly-generated key:
//!
//! ```sh
//! openssl rand -base64 32 | tr '+/' '-_' | tr -d '='
//! ```
//!
//! Key rotation is out of scope for Phase 6.  Until rotation is implemented,
//! changing the key requires a one-time re-encryption of all `enc:` values.
//!
//! # Limitations
//!
//! - Nonces are random (OsRng); no nonce reuse protection beyond statistical
//!   uniqueness.  At < 2^32 operations per key the birthday bound is safe.
//! - This module does **not** encrypt index or foreign-key columns; those
//!   must remain searchable at the SQL level.
//! - Audit log entries are NOT encrypted here; encrypting them would break
//!   the SHA-256 chain verification.

use aes_gcm::{
    aead::{Aead, AeadCore, KeyInit, OsRng},
    Aes256Gcm, Nonce,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};

use crate::errors::{AppError, AppResult};

const ENCRYPTED_PREFIX: &str = "enc:";
/// AES-256-GCM nonce length in bytes.
const NONCE_LEN: usize = 12;
/// AES-256-GCM authentication tag length in bytes.
const TAG_LEN: usize = 16;

/// Minimum byte length of a decoded encrypted value: nonce + empty payload + tag.
const MIN_ENCRYPTED_LEN: usize = NONCE_LEN + TAG_LEN;

/// Holds the 32-byte AES-256 key.
///
/// The struct is `Clone` so it can be stored in Rocket's managed state and
/// cloned into spawned tasks.
#[derive(Clone)]
pub struct FieldEncryption {
    key: [u8; 32],
}

impl std::fmt::Debug for FieldEncryption {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FieldEncryption")
            .field("key", &"[REDACTED]")
            .finish()
    }
}

impl FieldEncryption {
    /// Construct from a base64url (no-padding) encoded 32-byte key string.
    ///
    /// Returns an error if the string cannot be decoded or is not exactly
    /// 32 bytes long.
    pub fn from_base64(b64: &str) -> AppResult<Self> {
        // Accept both padded and unpadded input for resilience.
        let clean = b64.trim().trim_end_matches('=');
        let key_bytes = URL_SAFE_NO_PAD.decode(clean).map_err(|e| {
            AppError::Internal(format!("FIELD_ENCRYPTION_KEY: base64 decode failed: {}", e))
        })?;
        if key_bytes.len() != 32 {
            return Err(AppError::Internal(format!(
                "FIELD_ENCRYPTION_KEY must decode to exactly 32 bytes, got {}",
                key_bytes.len()
            )));
        }
        let mut key = [0u8; 32];
        key.copy_from_slice(&key_bytes);
        Ok(Self { key })
    }

    /// Returns `true` if this key is the known-insecure development default
    /// (32 zero bytes).  Callers should log a warning when this is true.
    pub fn is_dev_key(&self) -> bool {
        self.key == [0u8; 32]
    }

    /// Encrypt `plaintext` and return the `enc:<base64url>` storage value.
    pub fn encrypt(&self, plaintext: &str) -> AppResult<String> {
        let cipher = Aes256Gcm::new_from_slice(&self.key)
            .map_err(|e| AppError::Internal(format!("encryption cipher init: {}", e)))?;
        let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
        let ciphertext = cipher
            .encrypt(&nonce, plaintext.as_bytes())
            .map_err(|e| AppError::Internal(format!("AES-GCM encrypt: {}", e)))?;

        let mut combined = Vec::with_capacity(NONCE_LEN + ciphertext.len());
        combined.extend_from_slice(nonce.as_slice());
        combined.extend_from_slice(&ciphertext);
        Ok(format!("{}{}", ENCRYPTED_PREFIX, URL_SAFE_NO_PAD.encode(&combined)))
    }

    /// Decrypt a value produced by [`encrypt`].
    ///
    /// If `value` does not start with `enc:`, it is returned unchanged
    /// (plaintext passthrough for legacy rows).
    pub fn decrypt(&self, value: &str) -> AppResult<String> {
        let encoded = match value.strip_prefix(ENCRYPTED_PREFIX) {
            Some(e) => e,
            None => return Ok(value.to_string()), // legacy plaintext passthrough
        };
        let combined = URL_SAFE_NO_PAD.decode(encoded).map_err(|e| {
            AppError::Internal(format!("AES-GCM decrypt: base64 decode failed: {}", e))
        })?;
        if combined.len() < MIN_ENCRYPTED_LEN {
            return Err(AppError::Internal(
                "AES-GCM decrypt: encoded value too short to be valid ciphertext".to_string(),
            ));
        }
        let (nonce_bytes, ciphertext) = combined.split_at(NONCE_LEN);
        let cipher = Aes256Gcm::new_from_slice(&self.key)
            .map_err(|e| AppError::Internal(format!("encryption cipher init: {}", e)))?;
        let nonce = Nonce::from_slice(nonce_bytes);
        let plaintext = cipher.decrypt(nonce, ciphertext).map_err(|_| {
            // Do not leak internal details; authentication failure could indicate
            // key mismatch or tampering.
            AppError::Internal("AES-GCM decrypt: authentication failed".to_string())
        })?;
        String::from_utf8(plaintext)
            .map_err(|e| AppError::Internal(format!("AES-GCM decrypt: invalid UTF-8: {}", e)))
    }

    // ─── Optional helpers ────────────────────────────────────────────────────

    /// Encrypt `Some(value)`, returning `None` if the input is `None`.
    /// Empty strings are passed through unencrypted.
    pub fn encrypt_opt(&self, value: Option<&str>) -> AppResult<Option<String>> {
        match value {
            None => Ok(None),
            Some(v) if v.is_empty() => Ok(Some(String::new())),
            Some(v) => Ok(Some(self.encrypt(v)?)),
        }
    }

    /// Decrypt `Some(value)`, returning `None` if the input is `None`.
    pub fn decrypt_opt(&self, value: Option<&str>) -> AppResult<Option<String>> {
        match value {
            None => Ok(None),
            Some(v) if v.is_empty() => Ok(Some(String::new())),
            Some(v) => Ok(Some(self.decrypt(v)?)),
        }
    }

    /// Returns `true` if the string looks like an encrypted value.
    pub fn is_encrypted(s: &str) -> bool {
        s.starts_with(ENCRYPTED_PREFIX)
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::DEV_ENCRYPTION_KEY;

    fn dev_enc() -> FieldEncryption {
        FieldEncryption::from_base64(DEV_ENCRYPTION_KEY).unwrap()
    }

    #[test]
    fn encrypt_decrypt_roundtrip() {
        let enc = dev_enc();
        let plain = "Hello, Scholarly! 🔒";
        let cipher = enc.encrypt(plain).unwrap();
        assert!(cipher.starts_with("enc:"));
        let decoded = enc.decrypt(&cipher).unwrap();
        assert_eq!(decoded, plain);
    }

    #[test]
    fn different_nonces_produce_different_ciphertexts() {
        let enc = dev_enc();
        let c1 = enc.encrypt("same").unwrap();
        let c2 = enc.encrypt("same").unwrap();
        // Different nonces → different ciphertexts
        assert_ne!(c1, c2);
        // Both decrypt to same value
        assert_eq!(enc.decrypt(&c1).unwrap(), "same");
        assert_eq!(enc.decrypt(&c2).unwrap(), "same");
    }

    #[test]
    fn plaintext_passthrough_on_non_enc_prefix() {
        let enc = dev_enc();
        let plain = "legacy plaintext note";
        assert_eq!(enc.decrypt(plain).unwrap(), plain);
    }

    #[test]
    fn encrypt_opt_none_stays_none() {
        let enc = dev_enc();
        assert_eq!(enc.encrypt_opt(None).unwrap(), None);
        assert_eq!(enc.decrypt_opt(None).unwrap(), None);
    }

    #[test]
    fn encrypt_opt_roundtrip() {
        let enc = dev_enc();
        let v = Some("sensitive note");
        let encrypted = enc.encrypt_opt(v).unwrap();
        assert!(encrypted.as_deref().map(FieldEncryption::is_encrypted).unwrap_or(false));
        let decrypted = enc.decrypt_opt(encrypted.as_deref()).unwrap();
        assert_eq!(decrypted.as_deref(), v);
    }

    #[test]
    fn bad_base64_key_rejected() {
        assert!(FieldEncryption::from_base64("not_valid_base64!!!").is_err());
    }

    #[test]
    fn wrong_length_key_rejected() {
        // 16 bytes → 22 base64url chars — not 32 bytes
        let short = URL_SAFE_NO_PAD.encode([0u8; 16]);
        assert!(FieldEncryption::from_base64(&short).is_err());
    }

    #[test]
    fn dev_key_detected() {
        let enc = dev_enc();
        assert!(enc.is_dev_key());
    }

    #[test]
    fn real_key_not_flagged_as_dev() {
        let key = [1u8; 32]; // non-zero key
        let b64 = URL_SAFE_NO_PAD.encode(key);
        let enc = FieldEncryption::from_base64(&b64).unwrap();
        assert!(!enc.is_dev_key());
    }

    #[test]
    fn is_encrypted_predicate() {
        assert!(FieldEncryption::is_encrypted("enc:AAAAA"));
        assert!(!FieldEncryption::is_encrypted("plain text"));
        assert!(!FieldEncryption::is_encrypted(""));
    }
}
