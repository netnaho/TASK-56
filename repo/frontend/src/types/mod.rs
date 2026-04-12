//! Common types re-exported for use across the Scholarly frontend.
//!
//! Centralises domain types so that page components, hooks, and state
//! modules can import from one place.

use serde::{Deserialize, Serialize};

/// User roles in the Scholarly system.
///
/// Ordered from most privileged to least privileged.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Role {
    Admin,
    /// Compliance/audit role. Read-only access to audit logs and reports.
    Auditor,
    Librarian,
    DepartmentHead,
    Instructor,
    Viewer,
}

impl Role {
    /// Returns a numeric privilege level for comparison.
    ///
    /// Higher values indicate greater privilege.
    pub fn level(&self) -> u8 {
        match self {
            Role::Admin => 100,
            Role::Auditor => 90,
            Role::Librarian => 80,
            Role::DepartmentHead => 60,
            Role::Instructor => 40,
            Role::Viewer => 20,
        }
    }

    /// Parses a snake-case role string (as emitted by the backend) into a
    /// [`Role`]. Unknown values return `None`.
    pub fn from_str(value: &str) -> Option<Role> {
        match value {
            "admin" => Some(Role::Admin),
            "auditor" => Some(Role::Auditor),
            "librarian" => Some(Role::Librarian),
            "department_head" => Some(Role::DepartmentHead),
            "instructor" => Some(Role::Instructor),
            "viewer" => Some(Role::Viewer),
            _ => None,
        }
    }

    /// Returns the snake-case wire representation for this role.
    pub fn as_snake(&self) -> &'static str {
        match self {
            Role::Admin => "admin",
            Role::Auditor => "auditor",
            Role::Librarian => "librarian",
            Role::DepartmentHead => "department_head",
            Role::Instructor => "instructor",
            Role::Viewer => "viewer",
        }
    }
}

impl Default for Role {
    fn default() -> Self {
        Role::Viewer
    }
}

impl std::fmt::Display for Role {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let label = match self {
            Role::Admin => "Admin",
            Role::Auditor => "Auditor",
            Role::Librarian => "Librarian",
            Role::DepartmentHead => "Department Head",
            Role::Instructor => "Instructor",
            Role::Viewer => "Viewer",
        };
        write!(f, "{}", label)
    }
}

/// Represents an authenticated user in the system.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct User {
    /// Unique user identifier.
    pub id: String,
    /// Email address.
    pub email: String,
    /// Human-readable display name.
    pub display_name: String,
    /// Roles assigned to the user. The backend may assign multiple roles.
    pub roles: Vec<Role>,
    /// Department scoping, if any.
    pub department_id: Option<String>,
}
