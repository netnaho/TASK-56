//! Object-level authorization helpers.
//!
//! Separates policy from SQL. Service code calls these helpers to derive a
//! [`ScopeFilter`] describing which rows the principal may see, and the
//! repository layer translates it into a `WHERE` clause.
//!
//! Design intent: *nothing* in a route handler should hand-roll a
//! "if role == X then filter by Y" branch. All such decisions live here
//! and are unit-tested without a database.

use uuid::Uuid;

use super::principal::{Principal, Role};
use crate::errors::{AppError, AppResult};

/// A repository-agnostic description of which rows the caller may access.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScopeFilter {
    /// Unrestricted — the caller sees every row the domain exposes.
    All,
    /// Only rows whose `department_id` matches the caller's department.
    Department(Uuid),
    /// Only rows the caller personally owns (`owner_id == user_id`).
    OwnedBy(Uuid),
    /// Union of the above two: caller's department OR caller's own.
    DepartmentOrOwned { department_id: Uuid, owner_id: Uuid },
    /// Deny everything.
    None,
}

/// Derive the default content scope for a principal.
///
/// Rules (Phase 2):
/// * Admin sees everything.
/// * Librarian sees everything (library is institution-wide).
/// * Department Head sees rows in their department.
/// * Instructor sees their department's rows and anything they own.
/// * Viewer sees their department's rows only.
pub fn content_scope(principal: &Principal) -> ScopeFilter {
    if principal.is_admin() || principal.has_role(Role::Librarian) {
        return ScopeFilter::All;
    }
    match (principal.department_id, principal.has_role(Role::Instructor)) {
        (Some(dept), true) => ScopeFilter::DepartmentOrOwned {
            department_id: dept,
            owner_id: principal.user_id,
        },
        (Some(dept), false) => ScopeFilter::Department(dept),
        (None, true) => ScopeFilter::OwnedBy(principal.user_id),
        (None, false) => ScopeFilter::None,
    }
}

/// Assert that the given object is visible to the principal under `scope`.
///
/// `owner_id` and `department_id` are the object's attributes. Used by
/// service methods that fetch a row by id and then verify visibility
/// before returning it.
pub fn require_object_visible(
    scope: &ScopeFilter,
    object_owner_id: Option<Uuid>,
    object_department_id: Option<Uuid>,
) -> AppResult<()> {
    match scope {
        ScopeFilter::All => Ok(()),
        ScopeFilter::None => Err(AppError::Forbidden),
        ScopeFilter::Department(dept) => {
            if object_department_id == Some(*dept) {
                Ok(())
            } else {
                Err(AppError::Forbidden)
            }
        }
        ScopeFilter::OwnedBy(uid) => {
            if object_owner_id == Some(*uid) {
                Ok(())
            } else {
                Err(AppError::Forbidden)
            }
        }
        ScopeFilter::DepartmentOrOwned {
            department_id,
            owner_id,
        } => {
            if object_department_id == Some(*department_id)
                || object_owner_id == Some(*owner_id)
            {
                Ok(())
            } else {
                Err(AppError::Forbidden)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn principal(roles: Vec<Role>, dept: Option<Uuid>, uid: Uuid) -> Principal {
        Principal {
            user_id: uid,
            session_id: Uuid::new_v4(),
            email: "t@t".into(),
            display_name: "T".into(),
            roles,
            department_id: dept,
        }
    }

    #[test]
    fn admin_scope_is_all() {
        let p = principal(vec![Role::Admin], None, Uuid::new_v4());
        assert_eq!(content_scope(&p), ScopeFilter::All);
    }

    #[test]
    fn department_head_is_scoped_to_department() {
        let dept = Uuid::new_v4();
        let p = principal(vec![Role::DepartmentHead], Some(dept), Uuid::new_v4());
        assert_eq!(content_scope(&p), ScopeFilter::Department(dept));
    }

    #[test]
    fn instructor_sees_department_or_owned() {
        let dept = Uuid::new_v4();
        let uid = Uuid::new_v4();
        let p = principal(vec![Role::Instructor], Some(dept), uid);
        assert_eq!(
            content_scope(&p),
            ScopeFilter::DepartmentOrOwned {
                department_id: dept,
                owner_id: uid
            }
        );
    }

    #[test]
    fn object_visibility_department_match() {
        let dept = Uuid::new_v4();
        let scope = ScopeFilter::Department(dept);
        assert!(require_object_visible(&scope, None, Some(dept)).is_ok());
        assert!(require_object_visible(&scope, None, Some(Uuid::new_v4())).is_err());
        assert!(require_object_visible(&scope, None, None).is_err());
    }

    #[test]
    fn object_visibility_owned_match() {
        let uid = Uuid::new_v4();
        let scope = ScopeFilter::OwnedBy(uid);
        assert!(require_object_visible(&scope, Some(uid), None).is_ok());
        assert!(require_object_visible(&scope, Some(Uuid::new_v4()), None).is_err());
    }

    #[test]
    fn none_scope_always_denies() {
        assert!(require_object_visible(
            &ScopeFilter::None,
            Some(Uuid::new_v4()),
            Some(Uuid::new_v4())
        )
        .is_err());
    }

    #[test]
    fn viewer_with_no_department_is_blocked_from_everything() {
        let p = principal(vec![Role::Viewer], None, Uuid::new_v4());
        assert_eq!(content_scope(&p), ScopeFilter::None);
    }
}
