use serde::Deserialize;

/// Default dev encryption key: base64url of 32 zero bytes (URL_SAFE_NO_PAD).
/// This is a known-insecure sentinel for development only.
pub const DEV_ENCRYPTION_KEY: &str = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";

/// Central application configuration, loaded from environment variables.
///
/// # Phase 6 additions
/// - `field_encryption_key` — AES-256-GCM key for sensitive field encryption.
///   Generate a production key with `openssl rand -base64 32 | tr '+/' '-_' | tr -d '='`.
/// - `reports_storage_path` — where generated report artifact files are written.
#[derive(Debug, Clone, Deserialize)]
pub struct AppConfig {
    pub database_url: String,
    pub attachment_storage_path: String,
    pub jwt_secret: String,
    pub jwt_expiration_hours: u64,
    pub max_failed_logins: u32,
    pub lockout_duration_minutes: u32,
    /// Base64url-encoded (no padding) 32-byte key for AES-256-GCM field encryption.
    pub field_encryption_key: String,
    /// Directory where generated report artifact files are stored.
    pub reports_storage_path: String,
}

impl AppConfig {
    /// Load configuration from environment variables with sensible defaults.
    pub fn from_env() -> Self {
        let attachment_storage_path = std::env::var("ATTACHMENT_STORAGE_PATH")
            .unwrap_or_else(|_| "/data/attachments".to_string());
        let reports_storage_path = std::env::var("REPORTS_STORAGE_PATH")
            .unwrap_or_else(|_| format!("{}/reports", attachment_storage_path));
        Self {
            database_url: std::env::var("DATABASE_URL")
                .unwrap_or_else(|_| "mysql://scholarly_app:scholarly_app_pass@localhost:3306/scholarly".to_string()),
            attachment_storage_path,
            jwt_secret: std::env::var("JWT_SECRET")
                .unwrap_or_else(|_| "CHANGE_ME_IN_PRODUCTION".to_string()),
            jwt_expiration_hours: std::env::var("JWT_EXPIRATION_HOURS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(8),
            max_failed_logins: std::env::var("MAX_FAILED_LOGINS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(5),
            lockout_duration_minutes: std::env::var("LOCKOUT_DURATION_MINUTES")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(15),
            field_encryption_key: std::env::var("FIELD_ENCRYPTION_KEY")
                .unwrap_or_else(|_| DEV_ENCRYPTION_KEY.to_string()),
            reports_storage_path,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config_loads() {
        let config = AppConfig::from_env();
        assert!(!config.database_url.is_empty());
        assert!(!config.attachment_storage_path.is_empty());
    }

    #[test]
    fn test_reports_path_defaults_under_attachments() {
        std::env::remove_var("ATTACHMENT_STORAGE_PATH");
        std::env::remove_var("REPORTS_STORAGE_PATH");
        let config = AppConfig::from_env();
        assert!(config.reports_storage_path.starts_with(&config.attachment_storage_path));
    }

    /// Requirement: default lockout window is 15 minutes (not 30).
    #[test]
    fn default_lockout_duration_is_15_minutes() {
        std::env::remove_var("LOCKOUT_DURATION_MINUTES");
        let config = AppConfig::from_env();
        assert_eq!(
            config.lockout_duration_minutes, 15,
            "default lockout duration must be 15 minutes per policy"
        );
    }

    /// Requirement: env override wins over the compiled-in default.
    #[test]
    fn lockout_duration_env_override_takes_precedence() {
        std::env::set_var("LOCKOUT_DURATION_MINUTES", "60");
        let config = AppConfig::from_env();
        std::env::remove_var("LOCKOUT_DURATION_MINUTES");
        assert_eq!(
            config.lockout_duration_minutes, 60,
            "LOCKOUT_DURATION_MINUTES env var must override the default"
        );
    }

    /// Requirement: default failed-login threshold is 5 attempts.
    #[test]
    fn default_max_failed_logins_is_5() {
        std::env::remove_var("MAX_FAILED_LOGINS");
        let config = AppConfig::from_env();
        assert_eq!(
            config.max_failed_logins, 5,
            "default max failed logins must be 5"
        );
    }
}
