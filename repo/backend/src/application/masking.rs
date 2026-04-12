//! Response masking utilities for sensitive fields.
//!
//! The masking framework is deliberately explicit: every callsite that
//! returns possibly-sensitive data must pass its value through a masking
//! function. There is no implicit serde-time magic; a reviewer can grep
//! for `mask_` and see every redaction point.
//!
//! Current redaction rules (Phase 2):
//!
//! | Field                   | Visible to                                    |
//! |-------------------------|-----------------------------------------------|
//! | student identifier      | Admin, Instructor (their own section), DeptHead |
//! | instructor notes        | Admin, the instructor that wrote them          |
//! | actor email (audit log) | Admin only; others see a hashed prefix         |
//! | ip address (audit log)  | Admin only                                     |
//!
//! Object-level scope (who "their own section" is) is handled by
//! [`crate::application::scope`]; this module only answers the binary
//! "mask or not?" question given a capability gate.

use super::principal::{Principal, Role};
use sha2::{Digest, Sha256};

/// Generic masking helper: returns the original or `"[REDACTED]"`.
pub fn mask_if(value: &str, visible: bool) -> String {
    if visible {
        value.to_string()
    } else {
        "[REDACTED]".to_string()
    }
}

/// Mask an email for non-admin audit viewers.
///
/// Non-admins see a deterministic 8-char prefix of SHA-256(email) so the
/// audit log is still useful for correlation without leaking the identity.
pub fn mask_email_for_audit(email: &str, viewer: &Principal) -> String {
    if viewer.is_admin() {
        return email.to_string();
    }
    let digest = Sha256::digest(email.as_bytes());
    let hex = hex::encode(digest);
    format!("user:{}", &hex[..8])
}

/// Mask an IP address for non-admin audit viewers.
pub fn mask_ip_for_audit(ip: Option<&str>, viewer: &Principal) -> Option<String> {
    match ip {
        None => None,
        Some(ip) if viewer.is_admin() => Some(ip.to_string()),
        Some(_) => Some("[REDACTED]".to_string()),
    }
}

/// Is the viewer allowed to see raw student identifiers (id, email) on a row
/// owned by the given instructor / belonging to the given department?
pub fn can_see_student_identifier(
    viewer: &Principal,
    row_instructor_id: Option<uuid::Uuid>,
    row_department_id: Option<uuid::Uuid>,
) -> bool {
    if viewer.is_admin() {
        return true;
    }
    if viewer.has_role(Role::DepartmentHead) && viewer.department_id == row_department_id {
        return true;
    }
    if viewer.has_role(Role::Instructor) && Some(viewer.user_id) == row_instructor_id {
        return true;
    }
    false
}

/// Instructor notes: only the author and admins see them verbatim.
pub fn can_see_instructor_notes(viewer: &Principal, note_author_id: uuid::Uuid) -> bool {
    viewer.is_admin() || viewer.user_id == note_author_id
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn p(roles: Vec<Role>, dept: Option<Uuid>) -> Principal {
        Principal {
            user_id: Uuid::new_v4(),
            session_id: Uuid::new_v4(),
            email: "t@t".into(),
            display_name: "t".into(),
            roles,
            department_id: dept,
        }
    }

    #[test]
    fn mask_email_admin_sees_clear() {
        let admin = p(vec![Role::Admin], None);
        assert_eq!(mask_email_for_audit("a@b.com", &admin), "a@b.com");
    }

    #[test]
    fn mask_email_non_admin_sees_hash() {
        let viewer = p(vec![Role::Librarian], None);
        let masked = mask_email_for_audit("a@b.com", &viewer);
        assert!(masked.starts_with("user:"));
        assert_eq!(masked.len(), "user:".len() + 8);
        // Deterministic — same input gives same hash prefix.
        assert_eq!(mask_email_for_audit("a@b.com", &viewer), masked);
        // Different input gives different hash.
        assert_ne!(mask_email_for_audit("c@d.com", &viewer), masked);
    }

    #[test]
    fn mask_ip_non_admin_redacted() {
        let admin = p(vec![Role::Admin], None);
        let viewer = p(vec![Role::Librarian], None);
        assert_eq!(
            mask_ip_for_audit(Some("10.0.0.1"), &admin),
            Some("10.0.0.1".into())
        );
        assert_eq!(
            mask_ip_for_audit(Some("10.0.0.1"), &viewer),
            Some("[REDACTED]".into())
        );
        assert_eq!(mask_ip_for_audit(None, &admin), None);
    }

    #[test]
    fn student_identifier_visibility_rules() {
        let dept = Some(Uuid::new_v4());
        let instr_id = Uuid::new_v4();

        let admin = p(vec![Role::Admin], None);
        assert!(can_see_student_identifier(&admin, Some(instr_id), dept));

        let dh = p(vec![Role::DepartmentHead], dept);
        assert!(can_see_student_identifier(&dh, Some(instr_id), dept));

        let dh_wrong_dept = p(vec![Role::DepartmentHead], Some(Uuid::new_v4()));
        assert!(!can_see_student_identifier(&dh_wrong_dept, Some(instr_id), dept));

        let viewer = p(vec![Role::Viewer], dept);
        assert!(!can_see_student_identifier(&viewer, Some(instr_id), dept));
    }

    #[test]
    fn instructor_notes_only_author_or_admin() {
        let author_id = Uuid::new_v4();
        let author = Principal {
            user_id: author_id,
            session_id: Uuid::new_v4(),
            email: "i@i".into(),
            display_name: "I".into(),
            roles: vec![Role::Instructor],
            department_id: None,
        };
        let other = p(vec![Role::Instructor], None);
        let admin = p(vec![Role::Admin], None);

        assert!(can_see_instructor_notes(&author, author_id));
        assert!(!can_see_instructor_notes(&other, author_id));
        assert!(can_see_instructor_notes(&admin, author_id));
    }
}
