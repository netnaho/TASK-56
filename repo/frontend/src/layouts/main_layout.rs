//! Main application layout — top bar + sidebar navigation + content
//! area.
//!
//! The layout is role-aware: navigation items declare the minimum role
//! required to see them, and the sidebar filters items accordingly.
//! Unauthenticated users are redirected to `/login` via a side effect.

use dioxus::prelude::*;
use dioxus_router::prelude::*;

use crate::api::auth;
use crate::router::AppRoute;
use crate::state::AuthState;
use crate::types::Role;

// ---------------------------------------------------------------------------
// Navigation definition
// ---------------------------------------------------------------------------

/// A single item in the sidebar navigation.
#[derive(Clone, Debug)]
pub struct NavItem {
    /// Display label shown in the sidebar.
    pub label: &'static str,
    /// Route this item navigates to.
    pub route: AppRoute,
    /// Optional section heading rendered above this item when it is the
    /// first entry in a logical group.
    pub section: Option<&'static str>,
}

/// Returns the full ordered list of navigation items for the sidebar.
///
/// Items are grouped into logical sections via the `section` field.
pub fn navigation_items() -> Vec<NavItem> {
    vec![
        NavItem {
            label: "Dashboard",
            route: AppRoute::Dashboard {},
            section: None,
        },
        // -- Library --
        NavItem {
            label: "Journals",
            route: AppRoute::JournalList {},
            section: Some("Library"),
        },
        NavItem {
            label: "Resources",
            route: AppRoute::ResourceList {},
            section: None,
        },
        // -- Courses --
        NavItem {
            label: "Courses",
            route: AppRoute::CourseList {},
            section: Some("Courses"),
        },
        // -- Operations --
        NavItem {
            label: "Import / Export",
            route: AppRoute::Imports {},
            section: Some("Operations"),
        },
        NavItem {
            label: "Check-In",
            route: AppRoute::CheckIn {},
            section: None,
        },
        // -- Analytics --
        NavItem {
            label: "Reports",
            route: AppRoute::Reports {},
            section: Some("Analytics"),
        },
        NavItem {
            label: "Metrics",
            route: AppRoute::Metrics {},
            section: None,
        },
        NavItem {
            label: "Audit Logs",
            route: AppRoute::AuditLogs {},
            section: None,
        },
        // -- Admin --
        NavItem {
            label: "Settings",
            route: AppRoute::AdminSettings {},
            section: Some("Admin"),
        },
        NavItem {
            label: "Retention",
            route: AppRoute::RetentionSettings {},
            section: None,
        },
    ]
}

// ---------------------------------------------------------------------------
// Helper — explicit per-route access check
// ---------------------------------------------------------------------------

/// Returns `true` if at least one of `roles` grants access to the sidebar
/// item that navigates to `route`.
///
/// The allowlist is intentionally explicit rather than level-based so that
/// non-hierarchical access patterns (e.g. Auditor sees Audit Logs but not
/// Check-In; Instructor sees Check-In but not Import/Export) are clearly
/// visible and testable.
///
/// Routes that are not listed here (detail pages, login, etc.) are not
/// displayed in the sidebar, so they do not need an entry.
pub fn nav_item_allowed(roles: &[Role], route: &AppRoute) -> bool {
    let has = |r: &Role| roles.contains(r);
    match route {
        // Visible to every authenticated user.
        AppRoute::Dashboard {}
        | AppRoute::JournalList {}
        | AppRoute::ResourceList {}
        | AppRoute::CourseList {} => true,

        // Import / Export — Admin, Librarian, DepartmentHead.
        AppRoute::Imports {} => {
            has(&Role::Admin) || has(&Role::Librarian) || has(&Role::DepartmentHead)
        }

        // Check-In — Admin, DepartmentHead, Instructor.
        AppRoute::CheckIn {} => {
            has(&Role::Admin) || has(&Role::DepartmentHead) || has(&Role::Instructor)
        }

        // Reports — Admin, Auditor, Librarian, DepartmentHead, Instructor.
        AppRoute::Reports {} => {
            has(&Role::Admin)
                || has(&Role::Auditor)
                || has(&Role::Librarian)
                || has(&Role::DepartmentHead)
                || has(&Role::Instructor)
        }

        // Metrics — Admin, Librarian, DepartmentHead, Instructor.
        AppRoute::Metrics {} => {
            has(&Role::Admin)
                || has(&Role::Librarian)
                || has(&Role::DepartmentHead)
                || has(&Role::Instructor)
        }

        // Audit Logs — Admin and Auditor only.
        AppRoute::AuditLogs {} => has(&Role::Admin) || has(&Role::Auditor),

        // Admin-only routes.
        AppRoute::AdminSettings {} | AppRoute::RetentionSettings {} => has(&Role::Admin),

        // All other routes (detail pages, login, etc.) are not sidebar items.
        _ => false,
    }
}

// ---------------------------------------------------------------------------
// Layout component
// ---------------------------------------------------------------------------

/// Main layout component rendered around all authenticated routes.
///
/// Provides the top bar, sidebar navigation, and a content area that is
/// filled by the active child route via `Outlet`.
#[component]
pub fn MainLayout() -> Element {
    let mut auth_state = use_context::<Signal<AuthState>>();
    let navigator = use_navigator();

    // Redirect unauthenticated visitors to /login. Running the push
    // inside `use_effect` avoids mutating the router during rendering.
    let authed = auth_state.read().is_authenticated();
    use_effect(move || {
        if !auth_state.read().is_authenticated() {
            navigator.push(AppRoute::Login {});
        }
    });

    if !authed {
        // Render an empty placeholder while the effect pushes the
        // user to the login page on the next tick.
        return rsx! { div {} };
    }

    let snapshot = auth_state.read().clone();
    let roles: Vec<Role> = snapshot
        .user
        .as_ref()
        .map(|u| u.roles.clone())
        .unwrap_or_default();
    let primary_role = snapshot.primary_role().unwrap_or(Role::Viewer);
    let display_name = snapshot
        .user
        .as_ref()
        .map(|u| u.display_name.clone())
        .unwrap_or_else(|| "Unknown user".to_string());

    let nav_items = navigation_items();

    let on_logout = move |_| {
        let token = auth_state
            .read()
            .token
            .clone()
            .unwrap_or_default();
        spawn(async move {
            // Best-effort server-side logout — even if the call fails
            // (e.g. already-expired token), we still clear local state.
            let _ = auth::logout(&token).await;
            auth_state.set(AuthState::default());
            AuthState::clear_storage();
            navigator.push(AppRoute::Login {});
        });
    };

    rsx! {
        div { class: "app-shell",
            // ── Sidebar ──────────────────────────────────────
            nav { class: "sidebar",
                div { class: "sidebar__brand", "Scholarly" }

                div { class: "sidebar__nav",
                    for item in nav_items.iter() {
                        if nav_item_allowed(&roles, &item.route) {
                            if let Some(section) = item.section {
                                div { class: "sidebar__section-title", "{section}" }
                            }

                            Link {
                                class: "sidebar__link",
                                to: item.route.clone(),
                                "{item.label}"
                            }
                        }
                    }
                }
            }

            // ── Main column ──────────────────────────────────
            div { class: "app-shell__main",
                header { class: "topbar",
                    div { class: "topbar__brand", "Scholarly" }
                    div { class: "topbar__user",
                        span { class: "topbar__user-name", "{display_name}" }
                        span {
                            class: "role-badge",
                            "data-role": "{primary_role.as_snake()}",
                            "{primary_role}"
                        }
                        button {
                            class: "topbar__logout",
                            r#type: "button",
                            onclick: on_logout,
                            "Log out"
                        }
                    }
                }

                main { class: "main-content",
                    Outlet::<AppRoute> {}
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // ── helpers ──────────────────────────────────────────────────────────────

    fn allowed(roles: &[Role], route: &AppRoute) -> bool {
        nav_item_allowed(roles, route)
    }

    fn admin() -> Vec<Role> {
        vec![Role::Admin]
    }
    fn auditor() -> Vec<Role> {
        vec![Role::Auditor]
    }
    fn librarian() -> Vec<Role> {
        vec![Role::Librarian]
    }
    fn depthead() -> Vec<Role> {
        vec![Role::DepartmentHead]
    }
    fn instructor() -> Vec<Role> {
        vec![Role::Instructor]
    }
    fn viewer() -> Vec<Role> {
        vec![Role::Viewer]
    }

    // ── Admin ─────────────────────────────────────────────────────────────────

    #[test]
    fn admin_sees_all_nav_items() {
        let r = admin();
        assert!(allowed(&r, &AppRoute::Dashboard {}));
        assert!(allowed(&r, &AppRoute::JournalList {}));
        assert!(allowed(&r, &AppRoute::ResourceList {}));
        assert!(allowed(&r, &AppRoute::CourseList {}));
        assert!(allowed(&r, &AppRoute::Imports {}));
        assert!(allowed(&r, &AppRoute::CheckIn {}));
        assert!(allowed(&r, &AppRoute::Reports {}));
        assert!(allowed(&r, &AppRoute::Metrics {}));
        assert!(allowed(&r, &AppRoute::AuditLogs {}));
        assert!(allowed(&r, &AppRoute::AdminSettings {}));
        assert!(allowed(&r, &AppRoute::RetentionSettings {}));
    }

    // ── Auditor ───────────────────────────────────────────────────────────────

    #[test]
    fn auditor_sees_dashboard_reports_and_audit_logs() {
        let r = auditor();
        assert!(allowed(&r, &AppRoute::Dashboard {}));
        assert!(allowed(&r, &AppRoute::JournalList {}));
        assert!(allowed(&r, &AppRoute::ResourceList {}));
        assert!(allowed(&r, &AppRoute::CourseList {}));
        assert!(allowed(&r, &AppRoute::Reports {}));
        assert!(allowed(&r, &AppRoute::AuditLogs {}));
    }

    #[test]
    fn auditor_cannot_see_operations_metrics_or_admin_routes() {
        let r = auditor();
        assert!(!allowed(&r, &AppRoute::Imports {}));
        assert!(!allowed(&r, &AppRoute::CheckIn {}));
        assert!(!allowed(&r, &AppRoute::Metrics {}));
        assert!(!allowed(&r, &AppRoute::AdminSettings {}));
        assert!(!allowed(&r, &AppRoute::RetentionSettings {}));
    }

    // ── Librarian ─────────────────────────────────────────────────────────────

    #[test]
    fn librarian_sees_imports_reports_metrics_but_not_audit_or_admin() {
        let r = librarian();
        assert!(allowed(&r, &AppRoute::Dashboard {}));
        assert!(allowed(&r, &AppRoute::JournalList {}));
        assert!(allowed(&r, &AppRoute::Imports {}));
        assert!(allowed(&r, &AppRoute::Reports {}));
        assert!(allowed(&r, &AppRoute::Metrics {}));
    }

    #[test]
    fn librarian_cannot_see_checkin_audit_or_admin_routes() {
        let r = librarian();
        assert!(!allowed(&r, &AppRoute::CheckIn {}));
        assert!(!allowed(&r, &AppRoute::AuditLogs {}));
        assert!(!allowed(&r, &AppRoute::AdminSettings {}));
        assert!(!allowed(&r, &AppRoute::RetentionSettings {}));
    }

    // ── DepartmentHead ────────────────────────────────────────────────────────

    #[test]
    fn depthead_sees_imports_checkin_reports_metrics() {
        let r = depthead();
        assert!(allowed(&r, &AppRoute::Dashboard {}));
        assert!(allowed(&r, &AppRoute::Imports {}));
        assert!(allowed(&r, &AppRoute::CheckIn {}));
        assert!(allowed(&r, &AppRoute::Reports {}));
        assert!(allowed(&r, &AppRoute::Metrics {}));
    }

    #[test]
    fn depthead_cannot_see_audit_logs_or_admin_routes() {
        let r = depthead();
        assert!(!allowed(&r, &AppRoute::AuditLogs {}));
        assert!(!allowed(&r, &AppRoute::AdminSettings {}));
        assert!(!allowed(&r, &AppRoute::RetentionSettings {}));
    }

    // ── Instructor ────────────────────────────────────────────────────────────

    #[test]
    fn instructor_sees_checkin_and_reports_but_not_imports() {
        let r = instructor();
        assert!(allowed(&r, &AppRoute::Dashboard {}));
        assert!(allowed(&r, &AppRoute::JournalList {}));
        assert!(allowed(&r, &AppRoute::CheckIn {}));
        assert!(allowed(&r, &AppRoute::Reports {}));
        assert!(allowed(&r, &AppRoute::Metrics {}));
    }

    #[test]
    fn instructor_cannot_see_imports_audit_or_admin_routes() {
        let r = instructor();
        assert!(!allowed(&r, &AppRoute::Imports {}));
        assert!(!allowed(&r, &AppRoute::AuditLogs {}));
        assert!(!allowed(&r, &AppRoute::AdminSettings {}));
        assert!(!allowed(&r, &AppRoute::RetentionSettings {}));
    }

    // ── Viewer ────────────────────────────────────────────────────────────────

    #[test]
    fn viewer_sees_only_common_routes() {
        let r = viewer();
        assert!(allowed(&r, &AppRoute::Dashboard {}));
        assert!(allowed(&r, &AppRoute::JournalList {}));
        assert!(allowed(&r, &AppRoute::ResourceList {}));
        assert!(allowed(&r, &AppRoute::CourseList {}));
    }

    #[test]
    fn viewer_cannot_see_any_restricted_route() {
        let r = viewer();
        assert!(!allowed(&r, &AppRoute::Imports {}));
        assert!(!allowed(&r, &AppRoute::CheckIn {}));
        assert!(!allowed(&r, &AppRoute::Reports {}));
        assert!(!allowed(&r, &AppRoute::Metrics {}));
        assert!(!allowed(&r, &AppRoute::AuditLogs {}));
        assert!(!allowed(&r, &AppRoute::AdminSettings {}));
        assert!(!allowed(&r, &AppRoute::RetentionSettings {}));
    }

    // ── Multi-role user ───────────────────────────────────────────────────────

    #[test]
    fn multi_role_user_gets_union_of_permissions() {
        // A user with both Auditor and Instructor roles should see Audit Logs
        // (from Auditor) and Check-In (from Instructor).
        let r = vec![Role::Auditor, Role::Instructor];
        assert!(allowed(&r, &AppRoute::AuditLogs {}));
        assert!(allowed(&r, &AppRoute::CheckIn {}));
        assert!(allowed(&r, &AppRoute::Reports {}));
        // But neither Auditor nor Instructor has access to Imports.
        assert!(!allowed(&r, &AppRoute::Imports {}));
    }

    // ── Empty roles ───────────────────────────────────────────────────────────

    #[test]
    fn no_roles_only_sees_common_routes() {
        let r: Vec<Role> = vec![];
        assert!(allowed(&r, &AppRoute::Dashboard {}));
        assert!(!allowed(&r, &AppRoute::AuditLogs {}));
        assert!(!allowed(&r, &AppRoute::AdminSettings {}));
    }
}
