//! Unit tests for `crate::types::Role` — covers privilege ordering, string
//! parsing, and display representation used throughout the frontend.
//!
//! These tests exercise the pure-Rust logic in `types/mod.rs` that guards
//! role-based UI decisions (e.g., which navigation items are visible).

use crate::types::Role;

// ---------------------------------------------------------------------------
// Privilege ordering
// ---------------------------------------------------------------------------

#[test]
fn admin_has_highest_privilege_level() {
    assert_eq!(Role::Admin.level(), 100);
}

#[test]
fn viewer_has_lowest_privilege_level() {
    assert_eq!(Role::Viewer.level(), 20);
}

#[test]
fn role_levels_are_strictly_ordered() {
    assert!(Role::Admin.level() > Role::Auditor.level());
    assert!(Role::Auditor.level() > Role::Librarian.level());
    assert!(Role::Librarian.level() > Role::DepartmentHead.level());
    assert!(Role::DepartmentHead.level() > Role::Instructor.level());
    assert!(Role::Instructor.level() > Role::Viewer.level());
}

#[test]
fn highest_level_role_wins_in_max_comparison() {
    let roles = vec![Role::Viewer, Role::Admin, Role::Instructor];
    let highest = roles.iter().max_by_key(|r| r.level()).cloned();
    assert_eq!(highest, Some(Role::Admin));
}

// ---------------------------------------------------------------------------
// String parsing (from_str)
// ---------------------------------------------------------------------------

#[test]
fn from_str_parses_all_valid_role_strings() {
    assert_eq!(Role::from_str("admin"), Some(Role::Admin));
    assert_eq!(Role::from_str("auditor"), Some(Role::Auditor));
    assert_eq!(Role::from_str("librarian"), Some(Role::Librarian));
    assert_eq!(Role::from_str("department_head"), Some(Role::DepartmentHead));
    assert_eq!(Role::from_str("instructor"), Some(Role::Instructor));
    assert_eq!(Role::from_str("viewer"), Some(Role::Viewer));
}

#[test]
fn from_str_returns_none_for_unknown_roles() {
    assert_eq!(Role::from_str("superadmin"), None);
    assert_eq!(Role::from_str(""), None);
    assert_eq!(Role::from_str("Admin"), None); // case-sensitive
    assert_eq!(Role::from_str("VIEWER"), None);
}

// ---------------------------------------------------------------------------
// Wire representation (as_snake)
// ---------------------------------------------------------------------------

#[test]
fn as_snake_returns_correct_strings() {
    assert_eq!(Role::Admin.as_snake(), "admin");
    assert_eq!(Role::Auditor.as_snake(), "auditor");
    assert_eq!(Role::Librarian.as_snake(), "librarian");
    assert_eq!(Role::DepartmentHead.as_snake(), "department_head");
    assert_eq!(Role::Instructor.as_snake(), "instructor");
    assert_eq!(Role::Viewer.as_snake(), "viewer");
}

#[test]
fn as_snake_and_from_str_are_inverse_for_all_roles() {
    let all_roles = [
        Role::Admin,
        Role::Auditor,
        Role::Librarian,
        Role::DepartmentHead,
        Role::Instructor,
        Role::Viewer,
    ];
    for role in &all_roles {
        let snake = role.as_snake();
        assert_eq!(
            Role::from_str(snake),
            Some(role.clone()),
            "from_str(as_snake()) must round-trip for {:?}",
            role
        );
    }
}

// ---------------------------------------------------------------------------
// Display
// ---------------------------------------------------------------------------

#[test]
fn display_returns_human_readable_labels() {
    assert_eq!(format!("{}", Role::Admin), "Admin");
    assert_eq!(format!("{}", Role::DepartmentHead), "Department Head");
    assert_eq!(format!("{}", Role::Viewer), "Viewer");
}

// ---------------------------------------------------------------------------
// Default
// ---------------------------------------------------------------------------

#[test]
fn default_role_is_viewer() {
    assert_eq!(Role::default(), Role::Viewer);
}

// ---------------------------------------------------------------------------
// Equality and Clone
// ---------------------------------------------------------------------------

#[test]
fn role_equality_works() {
    assert_eq!(Role::Admin, Role::Admin);
    assert_ne!(Role::Admin, Role::Viewer);
}

#[test]
fn role_clone_produces_equal_value() {
    let r = Role::Instructor;
    let c = r.clone();
    assert_eq!(r, c);
}
