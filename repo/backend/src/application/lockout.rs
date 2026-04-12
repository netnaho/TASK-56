//! Failed-login tracking and account lockout enforcement.
//!
//! Policy (defaults, overridable via [`crate::config::AppConfig`]):
//!
//! * After **5 failed attempts within 15 minutes**, any further login for
//!   that email is rejected with `AppError::AccountLocked` until the
//!   oldest failure in the window ages out.
//! * Successful logins purge the failed-attempts history for that email.
//!
//! Lockout is scoped by **email only** (not `(email, ip)`) to prevent an
//! attacker from spreading attempts across source IPs to evade the limit.

use chrono::{Duration, Utc};
use sqlx::{MySqlPool, Row};
use uuid::Uuid;

use crate::config::AppConfig;
use crate::errors::{AppError, AppResult};

/// Returns `Ok(())` if the account is allowed to attempt login, or
/// `AppError::AccountLocked` if too many failures occurred recently.
pub async fn enforce_lockout(
    pool: &MySqlPool,
    email: &str,
    config: &AppConfig,
) -> AppResult<()> {
    let since = Utc::now().naive_utc()
        - Duration::minutes(config.lockout_duration_minutes as i64);

    let row = sqlx::query(
        r#"
        SELECT COUNT(*) AS failures
          FROM failed_login_attempts
         WHERE email = ?
           AND attempted_at >= ?
        "#,
    )
    .bind(email)
    .bind(since)
    .fetch_one(pool)
    .await
    .map_err(|e| AppError::Database(format!("count_failed_attempts: {}", e)))?;

    let count: i64 = row
        .try_get("failures")
        .map_err(|e| AppError::Database(e.to_string()))?;

    if count >= config.max_failed_logins as i64 {
        return Err(AppError::AccountLocked(format!(
            "account temporarily locked after {} failed login attempts; retry in at most {} minutes",
            count, config.lockout_duration_minutes
        )));
    }
    Ok(())
}

/// Record a failed login attempt. Reasons are free-form but should be a
/// short enumerated string (`invalid_password`, `unknown_user`, `locked`, ...).
pub async fn record_failure(
    pool: &MySqlPool,
    email: &str,
    ip_address: Option<&str>,
    reason: &str,
) -> AppResult<()> {
    sqlx::query(
        r#"
        INSERT INTO failed_login_attempts (id, email, ip_address, reason)
        VALUES (?, ?, ?, ?)
        "#,
    )
    .bind(Uuid::new_v4().to_string())
    .bind(email)
    .bind(ip_address)
    .bind(reason)
    .execute(pool)
    .await
    .map_err(|e| AppError::Database(format!("record_failed_login: {}", e)))?;
    Ok(())
}

/// Purge failed-attempts history for an email. Called immediately after a
/// successful login so a user never stays "half-locked".
pub async fn clear_failures(pool: &MySqlPool, email: &str) -> AppResult<()> {
    sqlx::query("DELETE FROM failed_login_attempts WHERE email = ?")
        .bind(email)
        .execute(pool)
        .await
        .map_err(|e| AppError::Database(format!("clear_failures: {}", e)))?;
    Ok(())
}

/// Pure function exposed for unit tests: does `count` within `window_minutes`
/// trigger lockout under `config`? Isolates the policy from the database.
pub fn is_locked(count: i64, config: &AppConfig) -> bool {
    count >= config.max_failed_logins as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> AppConfig {
        AppConfig {
            database_url: "".into(),
            attachment_storage_path: "".into(),
            jwt_secret: "".into(),
            jwt_expiration_hours: 8,
            max_failed_logins: 5,
            lockout_duration_minutes: 15,
            field_encryption_key: "".into(),
            reports_storage_path: "".into(),
        }
    }

    #[test]
    fn lockout_threshold_is_exclusive_below_five() {
        let cfg = test_config();
        assert!(!is_locked(0, &cfg));
        assert!(!is_locked(4, &cfg));
        assert!(is_locked(5, &cfg));
        assert!(is_locked(6, &cfg));
    }

    /// Policy constant: the test fixture window must be 15 minutes.
    #[test]
    fn lockout_policy_window_is_15_minutes() {
        let cfg = test_config();
        assert_eq!(
            cfg.lockout_duration_minutes, 15,
            "lockout window must be 15 minutes per policy"
        );
    }

    /// Policy constant: the threshold must be 5 failed attempts.
    #[test]
    fn lockout_policy_threshold_is_5_attempts() {
        let cfg = test_config();
        assert_eq!(
            cfg.max_failed_logins, 5,
            "lockout threshold must be 5 failed attempts per policy"
        );
    }
}
