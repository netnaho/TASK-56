//! The authenticated principal — what the auth guard puts into request state
//! and what every authorization helper receives.
//!
//! A `Principal` is always the result of a verified bearer token on an
//! active session. Unauthenticated callers never get a `Principal`; they
//! get `AppError::Unauthorized` from the guard.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// The fixed roles in the RBAC model. Stored in the database under
/// `roles.name` as lowercase snake_case.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Admin,
    Librarian,
    /// Cross-cutting read-only role for compliance and audit personnel.
    /// Grants audit log read and report read access; no write or admin
    /// capabilities. Level 90 — above Librarian, below Admin.
    Auditor,
    DepartmentHead,
    Instructor,
    Viewer,
}

impl Role {
    pub fn as_db_name(self) -> &'static str {
        match self {
            Role::Admin => "admin",
            Role::Librarian => "librarian",
            Role::Auditor => "auditor",
            Role::DepartmentHead => "department_head",
            Role::Instructor => "instructor",
            Role::Viewer => "viewer",
        }
    }

    pub fn from_db_name(name: &str) -> Option<Self> {
        match name {
            "admin" => Some(Role::Admin),
            "librarian" => Some(Role::Librarian),
            "auditor" => Some(Role::Auditor),
            "department_head" => Some(Role::DepartmentHead),
            "instructor" => Some(Role::Instructor),
            "viewer" => Some(Role::Viewer),
            _ => None,
        }
    }

    /// Monotonic privilege level, used for hierarchy-style checks
    /// (e.g. "any role at least as privileged as Librarian").
    pub fn level(self) -> u8 {
        match self {
            Role::Admin => 100,
            Role::Auditor => 90,
            Role::Librarian => 80,
            Role::DepartmentHead => 60,
            Role::Instructor => 40,
            Role::Viewer => 20,
        }
    }
}

/// Everything authorization code needs to know about the caller.
#[derive(Debug, Clone, Serialize)]
pub struct Principal {
    pub user_id: Uuid,
    pub session_id: Uuid,
    pub email: String,
    pub display_name: String,
    pub roles: Vec<Role>,
    pub department_id: Option<Uuid>,
}

impl Principal {
    pub fn has_role(&self, role: Role) -> bool {
        self.roles.iter().any(|r| *r == role)
    }

    pub fn is_admin(&self) -> bool {
        self.has_role(Role::Admin)
    }

    pub fn max_role_level(&self) -> u8 {
        self.roles.iter().map(|r| r.level()).max().unwrap_or(0)
    }

    /// True if any of the caller's roles meets the minimum level.
    pub fn at_least(&self, min: Role) -> bool {
        self.max_role_level() >= min.level()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn role_db_name_round_trips() {
        for r in [
            Role::Admin,
            Role::Librarian,
            Role::Auditor,
            Role::DepartmentHead,
            Role::Instructor,
            Role::Viewer,
        ] {
            assert_eq!(Role::from_db_name(r.as_db_name()), Some(r));
        }
        assert_eq!(Role::from_db_name("nope"), None);
    }

    #[test]
    fn level_ordering_matches_privilege() {
        assert!(Role::Admin.level() > Role::Auditor.level());
        assert!(Role::Auditor.level() > Role::Librarian.level());
        assert!(Role::Librarian.level() > Role::DepartmentHead.level());
        assert!(Role::DepartmentHead.level() > Role::Instructor.level());
        assert!(Role::Instructor.level() > Role::Viewer.level());
    }

    #[test]
    fn principal_at_least_respects_hierarchy() {
        let p = Principal {
            user_id: Uuid::nil(),
            session_id: Uuid::nil(),
            email: "x@y".into(),
            display_name: "x".into(),
            roles: vec![Role::Librarian],
            department_id: None,
        };
        assert!(p.at_least(Role::Viewer));
        assert!(p.at_least(Role::Instructor));
        assert!(p.at_least(Role::Librarian));
        assert!(!p.at_least(Role::Admin));
    }
}
