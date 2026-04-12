//! Password hashing and policy enforcement.
//!
//! Uses Argon2id with a per-password random salt. Hashes are stored in the
//! PHC string format (`$argon2id$v=19$...`) so algorithm parameters are
//! self-describing and future upgrades don't require schema changes.
//!
//! Policy:
//! * Minimum length: [`MIN_PASSWORD_LENGTH`] characters.
//! * The sentinel hash [`BOOTSTRAP_SENTINEL`] is rejected by `verify_password`
//!   — a user whose row still contains that value has never completed
//!   bootstrap and cannot log in.

use argon2::password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;

use crate::errors::AppError;

/// Minimum password length enforced at set/change time.
pub const MIN_PASSWORD_LENGTH: usize = 12;

/// Placeholder stored in seed rows so no credential material lives in SQL.
/// Replaced on first boot by [`crate::infrastructure::bootstrap`].
pub const BOOTSTRAP_SENTINEL: &str = "__BOOTSTRAP__";

/// Validate a plaintext password against policy. Does not hash.
pub fn validate_password_policy(password: &str) -> Result<(), AppError> {
    if password.chars().count() < MIN_PASSWORD_LENGTH {
        return Err(AppError::Validation(format!(
            "password must be at least {} characters long",
            MIN_PASSWORD_LENGTH
        )));
    }
    Ok(())
}

/// Hash a password with Argon2id using a fresh random salt.
///
/// Returns the PHC-formatted hash string ready for storage in
/// `users.password_hash`.
pub fn hash_password(password: &str) -> Result<String, AppError> {
    validate_password_policy(password)?;
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    argon2
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| AppError::Internal(format!("password hashing failed: {}", e)))
}

/// Constant-time verification of a plaintext password against a stored hash.
///
/// Returns `Ok(true)` on match, `Ok(false)` on mismatch, and an
/// `AppError::Internal` when the stored hash is malformed or is still the
/// bootstrap sentinel (i.e. the account has no real credential yet).
pub fn verify_password(password: &str, stored_hash: &str) -> Result<bool, AppError> {
    if stored_hash == BOOTSTRAP_SENTINEL {
        // Fail closed: a sentinel means bootstrap has not run yet.
        return Err(AppError::Internal(
            "account has no password set (bootstrap incomplete)".into(),
        ));
    }
    let parsed = PasswordHash::new(stored_hash)
        .map_err(|e| AppError::Internal(format!("stored hash is malformed: {}", e)))?;
    match Argon2::default().verify_password(password.as_bytes(), &parsed) {
        Ok(()) => Ok(true),
        Err(argon2::password_hash::Error::Password) => Ok(false),
        Err(e) => Err(AppError::Internal(format!("password verification failed: {}", e))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn policy_rejects_short_passwords() {
        assert!(validate_password_policy("short").is_err());
        assert!(validate_password_policy("elevenchars").is_err()); // 11 chars
        assert!(validate_password_policy("twelvecharss").is_ok()); // 12 chars
        assert!(validate_password_policy("ChangeMe!Scholarly2026").is_ok());
    }

    #[test]
    fn hash_and_verify_round_trip() {
        let password = "CorrectHorseBatteryStaple!";
        let hash = hash_password(password).expect("hashing works");
        assert!(hash.starts_with("$argon2id$"));
        assert!(verify_password(password, &hash).expect("verify works"));
        assert!(!verify_password("wrong-password-12", &hash).expect("verify works"));
    }

    #[test]
    fn bootstrap_sentinel_is_rejected() {
        let err = verify_password("anything-goes-here", BOOTSTRAP_SENTINEL).unwrap_err();
        match err {
            AppError::Internal(msg) => assert!(msg.contains("bootstrap")),
            _ => panic!("expected Internal error"),
        }
    }

    #[test]
    fn hashing_rejects_short_password() {
        assert!(hash_password("tooShort").is_err());
    }

    #[test]
    fn hash_uses_unique_salt() {
        // Two hashes of the same password must differ because of random salts.
        let h1 = hash_password("CorrectHorseBattery1").unwrap();
        let h2 = hash_password("CorrectHorseBattery1").unwrap();
        assert_ne!(h1, h2);
    }
}
