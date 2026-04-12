//! RoleBadge component — displays a coloured badge for a user role.

use dioxus::prelude::*;
use crate::types::Role;

/// Renders a small badge indicating the user's role.
///
/// TODO: Map each role to a distinct colour.
#[component]
pub fn RoleBadge(role: Role) -> Element {
    let label = match role {
        Role::Admin => "Admin",
        Role::Auditor => "Auditor",
        Role::Librarian => "Librarian",
        Role::Instructor => "Instructor",
        Role::DepartmentHead => "Dept. Head",
        Role::Viewer => "Viewer",
    };

    rsx! {
        span { class: "role-badge", "{label}" }
    }
}
