//! First-boot bootstrap routines.
//!
//! The seeds store users with `password_hash = '__BOOTSTRAP__'` so no
//! credential material lives in SQL files. On every startup we scan for
//! sentinel rows, hash the documented default password with Argon2id, and
//! overwrite the hash. This runs unconditionally but is idempotent: after
//! the first successful pass no sentinels remain and subsequent passes
//! are a no-op.
//!
//! The default password is documented in the README's "Default Seed Users"
//! section so operators know how to log in the first time.

use sqlx::{MySqlPool, Row};
use uuid::Uuid;

use crate::application::audit_service::{self, actions, AuditEvent};
use crate::application::password::{self, BOOTSTRAP_SENTINEL};
use crate::errors::{AppError, AppResult};

/// Default password assigned to seeded users on first boot.
///
/// Must be at least [`password::MIN_PASSWORD_LENGTH`] characters. See
/// the README — operators are expected to rotate these immediately.
pub const DEFAULT_SEED_PASSWORD: &str = "ChangeMe!Scholarly2026";

/// Replace every `__BOOTSTRAP__` sentinel in `users.password_hash` with a
/// real Argon2id hash of [`DEFAULT_SEED_PASSWORD`]. Returns the number of
/// rows updated.
pub async fn ensure_seed_passwords(pool: &MySqlPool) -> AppResult<u64> {
    let rows = sqlx::query("SELECT id, email FROM users WHERE password_hash = ?")
        .bind(BOOTSTRAP_SENTINEL)
        .fetch_all(pool)
        .await
        .map_err(|e| AppError::Database(format!("bootstrap scan: {}", e)))?;

    if rows.is_empty() {
        tracing::info!("bootstrap: no sentinel rows, skipping");
        return Ok(0);
    }

    let mut updated = 0u64;
    for row in rows {
        let id: String = row
            .try_get("id")
            .map_err(|e| AppError::Database(e.to_string()))?;
        let email: String = row
            .try_get("email")
            .map_err(|e| AppError::Database(e.to_string()))?;

        // Fresh hash per row (unique salt per user).
        let hash = password::hash_password(DEFAULT_SEED_PASSWORD)?;
        sqlx::query("UPDATE users SET password_hash = ? WHERE id = ?")
            .bind(&hash)
            .bind(&id)
            .execute(pool)
            .await
            .map_err(|e| AppError::Database(format!("bootstrap update: {}", e)))?;
        updated += 1;

        // Every bootstrap write is audited. The user_id is the subject.
        let uid = Uuid::parse_str(&id).map_err(|e| AppError::Database(e.to_string()))?;
        audit_service::record(
            pool,
            AuditEvent {
                actor_id: None,
                actor_email: Some(&email),
                action: actions::PASSWORD_BOOTSTRAP,
                target_entity_type: Some("user"),
                target_entity_id: Some(uid),
                change_payload: None,
                ip_address: None,
                user_agent: Some("system:bootstrap"),
            },
        )
        .await?;
    }

    tracing::warn!(
        updated,
        "bootstrap: replaced sentinel password hashes — rotate default credentials immediately"
    );
    Ok(updated)
}
