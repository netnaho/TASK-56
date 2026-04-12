//! Server-side session management.
//!
//! Authentication uses **opaque bearer tokens**, not JWTs:
//!
//! 1. On successful login, 32 random bytes are generated and base64url-encoded.
//!    This raw token is returned to the client exactly once.
//! 2. The server stores only `SHA-256(raw_token)` in `sessions.refresh_token_hash`.
//!    The raw token is never written to disk or logs.
//! 3. Subsequent requests carry `Authorization: Bearer <raw_token>`.
//!    The auth guard hashes the presented token and looks up the session row.
//!
//! Advantages over JWT for an offline deployment: instantaneous revocation,
//! no leaked key material, no clock-drift edge cases.

use chrono::{Duration, NaiveDateTime, Utc};
use rand::RngCore;
use sha2::{Digest, Sha256};
use sqlx::{MySqlPool, Row};
use uuid::Uuid;

use crate::errors::{AppError, AppResult};

/// Length of the raw token in bytes before base64 encoding.
const TOKEN_BYTES: usize = 32;

/// Result of creating a new session: the raw token goes to the client once,
/// the session id is used for revocation.
#[derive(Debug, Clone)]
pub struct IssuedSession {
    pub session_id: Uuid,
    pub raw_token: String,
    pub expires_at: NaiveDateTime,
}

/// Session row loaded by the auth guard. Mirrors the relevant columns of
/// the `sessions` table.
#[derive(Debug, Clone)]
pub struct SessionRecord {
    pub id: Uuid,
    pub user_id: Uuid,
    pub expires_at: NaiveDateTime,
    pub revoked_at: Option<NaiveDateTime>,
}

impl SessionRecord {
    pub fn is_active(&self, now: NaiveDateTime) -> bool {
        self.revoked_at.is_none() && self.expires_at > now
    }
}

/// Generate a fresh random token and return `(raw, sha256_hex)`.
pub fn generate_token() -> (String, String) {
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use base64::Engine as _;

    let mut bytes = [0u8; TOKEN_BYTES];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    let raw = URL_SAFE_NO_PAD.encode(bytes);
    let hash = hash_token(&raw);
    (raw, hash)
}

/// SHA-256 hex of a raw bearer token. Deterministic — used both to insert
/// on login and to look up on every authenticated request.
pub fn hash_token(raw: &str) -> String {
    let digest = Sha256::digest(raw.as_bytes());
    hex::encode(digest)
}

/// Create a new session row and return the raw token for one-time delivery.
pub async fn create_session(
    pool: &MySqlPool,
    user_id: Uuid,
    ip_address: Option<&str>,
    user_agent: Option<&str>,
    ttl_hours: i64,
) -> AppResult<IssuedSession> {
    let (raw_token, token_hash) = generate_token();
    let session_id = Uuid::new_v4();
    let expires_at = Utc::now().naive_utc() + Duration::hours(ttl_hours);

    sqlx::query(
        r#"
        INSERT INTO sessions (id, user_id, refresh_token_hash, ip_address, user_agent, expires_at)
        VALUES (?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(session_id.to_string())
    .bind(user_id.to_string())
    .bind(&token_hash)
    .bind(ip_address)
    .bind(user_agent)
    .bind(expires_at)
    .execute(pool)
    .await
    .map_err(|e| AppError::Database(format!("create_session: {}", e)))?;

    Ok(IssuedSession {
        session_id,
        raw_token,
        expires_at,
    })
}

/// Look up a session by the SHA-256 of the presented raw token.
/// Returns `Unauthorized` if no matching active session exists.
pub async fn find_active_by_token(
    pool: &MySqlPool,
    raw_token: &str,
) -> AppResult<SessionRecord> {
    let token_hash = hash_token(raw_token);

    let row = sqlx::query(
        r#"
        SELECT id, user_id, expires_at, revoked_at
          FROM sessions
         WHERE refresh_token_hash = ?
         LIMIT 1
        "#,
    )
    .bind(&token_hash)
    .fetch_optional(pool)
    .await
    .map_err(|e| AppError::Database(format!("find_session: {}", e)))?;

    let row = row.ok_or(AppError::Unauthorized)?;
    let id: String = row.try_get("id").map_err(|e| AppError::Database(e.to_string()))?;
    let user_id: String = row.try_get("user_id").map_err(|e| AppError::Database(e.to_string()))?;
    let expires_at: NaiveDateTime = row
        .try_get("expires_at")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let revoked_at: Option<NaiveDateTime> = row
        .try_get("revoked_at")
        .map_err(|e| AppError::Database(e.to_string()))?;

    let record = SessionRecord {
        id: Uuid::parse_str(&id).map_err(|e| AppError::Database(e.to_string()))?,
        user_id: Uuid::parse_str(&user_id).map_err(|e| AppError::Database(e.to_string()))?,
        expires_at,
        revoked_at,
    };

    if !record.is_active(Utc::now().naive_utc()) {
        return Err(AppError::Unauthorized);
    }
    Ok(record)
}

/// Revoke a session by id. Idempotent — revoking an already-revoked session
/// is not an error.
pub async fn revoke_session(pool: &MySqlPool, session_id: Uuid) -> AppResult<()> {
    sqlx::query(
        r#"
        UPDATE sessions
           SET revoked_at = COALESCE(revoked_at, NOW())
         WHERE id = ?
        "#,
    )
    .bind(session_id.to_string())
    .execute(pool)
    .await
    .map_err(|e| AppError::Database(format!("revoke_session: {}", e)))?;
    Ok(())
}

/// Revoke every active session belonging to a user. Used on password change.
pub async fn revoke_all_for_user(pool: &MySqlPool, user_id: Uuid) -> AppResult<()> {
    sqlx::query(
        r#"
        UPDATE sessions
           SET revoked_at = NOW()
         WHERE user_id = ? AND revoked_at IS NULL
        "#,
    )
    .bind(user_id.to_string())
    .execute(pool)
    .await
    .map_err(|e| AppError::Database(format!("revoke_all_for_user: {}", e)))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_token_is_unique_and_hashes_consistently() {
        let (raw1, hash1) = generate_token();
        let (raw2, hash2) = generate_token();
        assert_ne!(raw1, raw2, "raw tokens must differ");
        assert_ne!(hash1, hash2, "hashes must differ");
        assert_eq!(hash_token(&raw1), hash1, "hash is deterministic");
        // Raw token is base64url(32 bytes) == 43 chars, no padding.
        assert_eq!(raw1.len(), 43);
    }

    #[test]
    fn session_record_active_logic() {
        let now = Utc::now().naive_utc();
        let active = SessionRecord {
            id: Uuid::new_v4(),
            user_id: Uuid::new_v4(),
            expires_at: now + Duration::hours(1),
            revoked_at: None,
        };
        assert!(active.is_active(now));

        let expired = SessionRecord {
            expires_at: now - Duration::hours(1),
            ..active.clone()
        };
        assert!(!expired.is_active(now));

        let revoked = SessionRecord {
            revoked_at: Some(now),
            ..active
        };
        assert!(!revoked.is_active(now));
    }
}
