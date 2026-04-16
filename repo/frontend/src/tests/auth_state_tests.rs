//! Unit tests for `crate::state::AuthState` — covers struct construction,
//! serialization round-trip, and the pure-Rust `primary_role()` helper that
//! drives role-aware UI decisions.
//!
//! Tests here deliberately avoid calling `is_authenticated()` because that
//! method reads `js_sys::Date::now()`, which is a WASM runtime call and not
//! available in the native test environment.

use crate::state::AuthState;
use crate::types::{Role, User};

// ---------------------------------------------------------------------------
// Default state
// ---------------------------------------------------------------------------

#[test]
fn default_auth_state_has_no_user_or_token() {
    let state = AuthState::default();
    assert!(state.user.is_none(), "default AuthState must have no user");
    assert!(state.token.is_none(), "default AuthState must have no token");
    assert!(
        state.expires_at.is_none(),
        "default AuthState must have no expiry"
    );
}

// ---------------------------------------------------------------------------
// primary_role()
// ---------------------------------------------------------------------------

#[test]
fn primary_role_returns_none_when_no_user() {
    let state = AuthState::default();
    assert_eq!(state.primary_role(), None);
}

#[test]
fn primary_role_returns_highest_level_role() {
    let state = AuthState {
        user: Some(User {
            id: "u1".to_string(),
            email: "test@scholarly.local".to_string(),
            display_name: "Test User".to_string(),
            roles: vec![Role::Instructor, Role::Admin, Role::Viewer],
            department_id: None,
        }),
        token: Some("tok".to_string()),
        expires_at: None,
    };
    assert_eq!(state.primary_role(), Some(Role::Admin));
}

#[test]
fn primary_role_with_single_role_returns_that_role() {
    let state = AuthState {
        user: Some(User {
            id: "u2".to_string(),
            email: "librarian@scholarly.local".to_string(),
            display_name: "Librarian".to_string(),
            roles: vec![Role::Librarian],
            department_id: None,
        }),
        token: Some("tok".to_string()),
        expires_at: None,
    };
    assert_eq!(state.primary_role(), Some(Role::Librarian));
}

#[test]
fn primary_role_with_empty_roles_returns_none() {
    let state = AuthState {
        user: Some(User {
            id: "u3".to_string(),
            email: "nobody@scholarly.local".to_string(),
            display_name: "No Role".to_string(),
            roles: vec![],
            department_id: None,
        }),
        token: Some("tok".to_string()),
        expires_at: None,
    };
    assert_eq!(state.primary_role(), None);
}

// ---------------------------------------------------------------------------
// Serialization round-trip
// ---------------------------------------------------------------------------

#[test]
fn auth_state_serializes_and_deserializes_correctly() {
    let original = AuthState {
        user: Some(User {
            id: "abc123".to_string(),
            email: "admin@scholarly.local".to_string(),
            display_name: "Admin User".to_string(),
            roles: vec![Role::Admin],
            department_id: None,
        }),
        token: Some("test-bearer-token".to_string()),
        expires_at: Some("2099-01-01T00:00:00Z".to_string()),
    };

    let json = serde_json::to_string(&original).expect("AuthState must serialize to JSON");
    let restored: AuthState =
        serde_json::from_str(&json).expect("AuthState must deserialize from JSON");

    assert_eq!(restored.token, original.token);
    assert_eq!(restored.expires_at, original.expires_at);
    assert_eq!(
        restored.user.as_ref().map(|u| u.id.as_str()),
        Some("abc123")
    );
}

#[test]
fn auth_state_default_serializes_to_empty_json() {
    let state = AuthState::default();
    let json = serde_json::to_string(&state).expect("default AuthState must serialize");
    let restored: AuthState =
        serde_json::from_str(&json).expect("default AuthState JSON must deserialize");
    assert_eq!(restored, state);
}

#[test]
fn auth_state_with_department_id_round_trips() {
    let original = AuthState {
        user: Some(User {
            id: "d1".to_string(),
            email: "depthead@scholarly.local".to_string(),
            display_name: "Dept Head".to_string(),
            roles: vec![Role::DepartmentHead],
            department_id: Some("dept-cs".to_string()),
        }),
        token: Some("tok".to_string()),
        expires_at: None,
    };
    let json = serde_json::to_string(&original).unwrap();
    let restored: AuthState = serde_json::from_str(&json).unwrap();
    assert_eq!(
        restored.user.as_ref().and_then(|u| u.department_id.as_deref()),
        Some("dept-cs")
    );
}
