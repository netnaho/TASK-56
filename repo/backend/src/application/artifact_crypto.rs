//! Per-artifact envelope encryption for report files.
//!
//! # Design
//!
//! Each generated report artifact is encrypted at rest with a unique
//! 32-byte Data Encryption Key (DEK).  The DEK is itself encrypted
//! (wrapped) by the master `FIELD_ENCRYPTION_KEY` via [`FieldEncryption`]
//! and stored in the `report_runs.artifact_dek` column.
//!
//! ## Cryptographic erasure
//!
//! On retention expiry the *only* operation required for guaranteed erasure
//! is deleting (NULLing) the `artifact_dek` database row.  Once the DEK is
//! gone the on-disk ciphertext is permanently irrecoverable — even on
//! Docker OverlayFS where zero-overwriting files is best-effort.
//!
//! The physical file is then removed best-effort as before.
//!
//! ## On-disk format
//!
//! ```text
//! <12-byte random nonce> || <AES-256-GCM ciphertext> || <16-byte GCM tag>
//! ```
//!
//! The file is raw binary (no base64 wrapper); the DEK is stored in DB in
//! `enc:<base64url>` format via [`FieldEncryption::encrypt`].

use aes_gcm::{
    aead::{Aead, AeadCore, KeyInit, OsRng},
    Aes256Gcm, Key, Nonce,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use rand::RngCore;

use crate::application::encryption::FieldEncryption;
use crate::errors::{AppError, AppResult};

/// AES-256-GCM nonce length in bytes.
const NONCE_LEN: usize = 12;
/// Minimum ciphertext size: empty plaintext produces 16-byte GCM tag only.
const MIN_CIPHERTEXT_LEN: usize = NONCE_LEN + 16;

// ─── DEK lifecycle ────────────────────────────────────────────────────────────

/// Generate a fresh 32-byte random DEK.
pub fn generate_dek() -> [u8; 32] {
    let mut dek = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut dek);
    dek
}

/// Wrap (encrypt) `dek` using the master key and return an `enc:<base64url>`
/// string suitable for storage in `report_runs.artifact_dek`.
///
/// The DEK bytes are base64url-encoded before encryption so that
/// [`FieldEncryption::encrypt`] (which takes `&str`) can process them.
pub fn wrap_dek(enc: &FieldEncryption, dek: &[u8; 32]) -> AppResult<String> {
    let dek_b64 = URL_SAFE_NO_PAD.encode(dek);
    enc.encrypt(&dek_b64)
}

/// Unwrap (decrypt) a wrapped DEK produced by [`wrap_dek`].
///
/// Returns the raw 32-byte DEK on success.
pub fn unwrap_dek(enc: &FieldEncryption, wrapped: &str) -> AppResult<[u8; 32]> {
    let dek_b64 = enc.decrypt(wrapped)?;
    let bytes = URL_SAFE_NO_PAD.decode(&dek_b64).map_err(|e| {
        AppError::Internal(format!("artifact_crypto: DEK base64 decode failed: {}", e))
    })?;
    if bytes.len() != 32 {
        return Err(AppError::Internal(format!(
            "artifact_crypto: unwrapped DEK is {} bytes, expected 32",
            bytes.len()
        )));
    }
    let mut dek = [0u8; 32];
    dek.copy_from_slice(&bytes);
    Ok(dek)
}

// ─── Artifact encryption / decryption ────────────────────────────────────────

/// Encrypt `plaintext` bytes with `dek` using AES-256-GCM.
///
/// Returns raw bytes in the format `nonce || ciphertext || gcm-tag`.
pub fn encrypt_artifact(dek: &[u8; 32], plaintext: &[u8]) -> AppResult<Vec<u8>> {
    let key = Key::<Aes256Gcm>::from_slice(dek);
    let cipher = Aes256Gcm::new(key);
    let nonce = Aes256Gcm::generate_nonce(&mut OsRng);

    // aes_gcm appends the 16-byte tag to the ciphertext.
    let ciphertext_with_tag = cipher
        .encrypt(&nonce, plaintext)
        .map_err(|e| AppError::Internal(format!("artifact_crypto: encrypt failed: {}", e)))?;

    let mut out = Vec::with_capacity(NONCE_LEN + ciphertext_with_tag.len());
    out.extend_from_slice(nonce.as_slice());
    out.extend_from_slice(&ciphertext_with_tag);
    Ok(out)
}

/// Decrypt bytes produced by [`encrypt_artifact`].
///
/// Input must be `nonce || ciphertext || gcm-tag` (raw binary, no base64).
pub fn decrypt_artifact(dek: &[u8; 32], data: &[u8]) -> AppResult<Vec<u8>> {
    if data.len() < MIN_CIPHERTEXT_LEN {
        return Err(AppError::Internal(
            "artifact_crypto: encrypted artifact is too short".to_string(),
        ));
    }
    let (nonce_bytes, ciphertext_with_tag) = data.split_at(NONCE_LEN);
    let key = Key::<Aes256Gcm>::from_slice(dek);
    let cipher = Aes256Gcm::new(key);
    let nonce = Nonce::from_slice(nonce_bytes);

    cipher
        .decrypt(nonce, ciphertext_with_tag)
        .map_err(|_| AppError::Internal("artifact_crypto: decrypt authentication failed".to_string()))
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
    fn dek_wrap_unwrap_roundtrip() {
        let enc = dev_enc();
        let dek = generate_dek();
        let wrapped = wrap_dek(&enc, &dek).unwrap();
        assert!(wrapped.starts_with("enc:"), "wrapped DEK must use enc: prefix");
        let recovered = unwrap_dek(&enc, &wrapped).unwrap();
        assert_eq!(dek, recovered, "roundtrip must recover original DEK");
    }

    #[test]
    fn artifact_encrypt_decrypt_roundtrip() {
        let dek = generate_dek();
        let plaintext = b"journal_id,title,status,created_at\nfoo,bar,draft,2026-01-01";
        let ciphertext = encrypt_artifact(&dek, plaintext).unwrap();
        assert_ne!(ciphertext, plaintext, "ciphertext must differ from plaintext");
        let recovered = decrypt_artifact(&dek, &ciphertext).unwrap();
        assert_eq!(recovered, plaintext, "decrypt must recover original bytes");
    }

    #[test]
    fn wrong_dek_fails_authentication() {
        let dek1 = generate_dek();
        let dek2 = generate_dek();
        let plaintext = b"sensitive data";
        let ciphertext = encrypt_artifact(&dek1, plaintext).unwrap();
        let result = decrypt_artifact(&dek2, &ciphertext);
        assert!(result.is_err(), "wrong DEK must fail authentication");
    }

    #[test]
    fn tampered_ciphertext_fails_authentication() {
        let dek = generate_dek();
        let plaintext = b"sensitive data";
        let mut ciphertext = encrypt_artifact(&dek, plaintext).unwrap();
        // Flip a byte in the ciphertext region (after the 12-byte nonce).
        if ciphertext.len() > NONCE_LEN {
            ciphertext[NONCE_LEN] ^= 0xFF;
        }
        let result = decrypt_artifact(&dek, &ciphertext);
        assert!(result.is_err(), "tampered ciphertext must fail authentication");
    }

    #[test]
    fn two_deks_are_distinct() {
        let d1 = generate_dek();
        let d2 = generate_dek();
        assert_ne!(d1, d2, "freshly generated DEKs must be distinct");
    }

    #[test]
    fn crypto_erase_path_returns_irrecoverable_after_dek_deletion() {
        // Simulate: retain the ciphertext on disk (not physically deleted) but
        // lose the DEK.  Decryption with a new random DEK must fail.
        let original_dek = generate_dek();
        let plaintext = b"report artifact";
        let ciphertext = encrypt_artifact(&original_dek, plaintext).unwrap();

        let lost_dek = generate_dek(); // DEK was erased; simulate with random bytes
        let result = decrypt_artifact(&lost_dek, &ciphertext);
        assert!(
            result.is_err(),
            "ciphertext with lost DEK must be irrecoverable"
        );
    }
}
