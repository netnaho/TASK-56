use serde::{Serialize, Deserialize};
use chrono::NaiveDateTime;
use uuid::Uuid;

/// A named role within the system (e.g. "Instructor", "Student", "Admin").
///
/// Roles aggregate a set of permissions and are assigned to users through the
/// `UserRole` join entity.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Role {
    pub id: Uuid,

    /// Machine-readable slug, e.g. "admin", "instructor", "student".
    pub name: String,

    /// Human-friendly label shown in admin screens.
    pub display_name: String,

    /// Optional long-form description of what the role grants.
    pub description: Option<String>,

    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

/// A discrete, enforceable capability (e.g. "journal:write", "course:publish").
///
/// Permissions are attached to roles and checked at authorisation boundaries.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Permission {
    pub id: Uuid,

    /// Colon-delimited permission key, e.g. "teaching_resource:create".
    pub key: String,

    /// Human-readable explanation of what this permission allows.
    pub description: Option<String>,

    pub created_at: NaiveDateTime,
}

/// Join entity linking a `User` to a `Role`.
///
/// A user may hold multiple roles simultaneously.
/// TODO: decide whether role assignments should be time-bounded (valid_from / valid_until) in phase 2.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserRole {
    pub id: Uuid,

    /// The user receiving the role.
    pub user_id: Uuid,

    /// The role being assigned.
    pub role_id: Uuid,

    /// Who granted this assignment.
    /// TODO: consider making this a foreign key to `User` in phase 2.
    pub assigned_by: Option<Uuid>,

    pub created_at: NaiveDateTime,
}
