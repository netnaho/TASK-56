use serde::{Serialize, Deserialize};
use chrono::NaiveDateTime;
use uuid::Uuid;

/// An active user session, typically backed by a refresh-token pair.
///
/// Sessions are created on successful authentication and invalidated on
/// logout or expiry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: Uuid,

    /// The authenticated user who owns this session.
    pub user_id: Uuid,

    /// Opaque refresh token stored alongside the session.
    /// TODO: confirm token format (JWT / opaque random) in phase 2.
    pub refresh_token_hash: String,

    /// IP address that initiated the session.
    pub ip_address: Option<String>,

    /// User-Agent header captured at login time.
    pub user_agent: Option<String>,

    /// When the session expires if not renewed.
    pub expires_at: NaiveDateTime,

    /// Explicit revocation timestamp; `None` means still valid.
    pub revoked_at: Option<NaiveDateTime>,

    pub created_at: NaiveDateTime,
}

/// Records a failed login attempt for rate-limiting and security monitoring.
///
/// The system uses these records to enforce account lockout policies and
/// to feed the audit trail.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailedLoginAttempt {
    pub id: Uuid,

    /// Email address that was used in the attempt (may not map to a real user).
    pub email: String,

    /// Source IP address of the request.
    pub ip_address: Option<String>,

    /// Free-text reason code, e.g. "invalid_password", "account_locked".
    /// TODO: consider making this an enum in phase 2.
    pub reason: String,

    pub attempted_at: NaiveDateTime,
}

/// Value object carrying the raw credentials submitted during login.
///
/// This struct is ephemeral — it is never persisted.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoginCredentials {
    /// The email address supplied by the user.
    pub email: String,

    /// The plaintext password supplied by the user.  Must be zeroised after use.
    /// TODO: evaluate using `secrecy::SecretString` in phase 2.
    pub password: String,
}
