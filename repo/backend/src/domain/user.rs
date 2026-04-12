use serde::{Serialize, Deserialize};
use chrono::NaiveDateTime;
use uuid::Uuid;

/// Represents the activation state of a user account.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum UserStatus {
    /// Account created but email not yet verified.
    PendingVerification,
    /// Fully active account.
    Active,
    /// Temporarily suspended by an administrator.
    Suspended,
    /// Soft-deleted; retained for audit trail purposes.
    Deactivated,
}

/// A registered user within the Scholarly platform.
///
/// Users may be students, instructors, or administrators.  The `status`
/// field governs what actions the account may perform; authorisation
/// details live in the `role` module.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    /// Primary key — surrogate UUID.
    pub id: Uuid,

    /// Unique email address used for authentication.
    pub email: String,

    /// Display name shown in the UI.
    pub display_name: String,

    /// Argon2id password hash.  Never serialised to external consumers.
    /// TODO: confirm hash algorithm and pepper strategy in phase 2.
    pub password_hash: String,

    /// Current account status.
    pub status: UserStatus,

    /// Optional URL or object-store key for the user's avatar image.
    /// TODO: decide on storage backend (S3 / local) in phase 2.
    pub avatar_url: Option<String>,

    /// Optional phone number for MFA or notifications.
    /// TODO: determine E.164 validation rules in phase 2.
    pub phone: Option<String>,

    /// Timestamp of the most recent successful login.
    pub last_login_at: Option<NaiveDateTime>,

    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}
