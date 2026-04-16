//! Unit tests for `crate::router::AppRoute` — verifies that the route enum
//! covers all expected application pages and that route variants can be
//! constructed and compared.
//!
//! Also exercises the nav-guard logic surface that determines which routes
//! are visible to which roles, using the same `nav_item_allowed` function
//! used by the main layout component.

use crate::router::AppRoute;
use crate::types::Role;

// ---------------------------------------------------------------------------
// Route variant existence — ensures all declared routes compile and match
// ---------------------------------------------------------------------------

#[test]
fn login_route_is_distinct_from_authenticated_routes() {
    let login = AppRoute::Login {};
    let dashboard = AppRoute::Dashboard {};
    assert_ne!(login, dashboard);
}

#[test]
fn journal_detail_captures_id_parameter() {
    let r = AppRoute::JournalDetail {
        id: "abc-123".to_string(),
    };
    match r {
        AppRoute::JournalDetail { id } => assert_eq!(id, "abc-123"),
        _ => panic!("JournalDetail route must capture the id segment"),
    }
}

#[test]
fn resource_detail_captures_id_parameter() {
    let r = AppRoute::ResourceDetail {
        id: "res-456".to_string(),
    };
    match r {
        AppRoute::ResourceDetail { id } => assert_eq!(id, "res-456"),
        _ => panic!("ResourceDetail route must capture the id segment"),
    }
}

#[test]
fn course_detail_captures_id_parameter() {
    let r = AppRoute::CourseDetail {
        id: "crs-789".to_string(),
    };
    match r {
        AppRoute::CourseDetail { id } => assert_eq!(id, "crs-789"),
        _ => panic!("CourseDetail route must capture the id segment"),
    }
}

#[test]
fn section_list_and_section_detail_are_different_routes() {
    let list = AppRoute::SectionList {
        id: "crs-1".to_string(),
    };
    let detail = AppRoute::SectionDetail {
        id: "sec-1".to_string(),
    };
    assert_ne!(list, detail);
}

#[test]
fn admin_settings_and_retention_settings_are_different_routes() {
    assert_ne!(AppRoute::AdminSettings {}, AppRoute::RetentionSettings {});
}

#[test]
fn forbidden_page_is_distinct_from_authenticated_routes() {
    assert_ne!(AppRoute::ForbiddenPage {}, AppRoute::Dashboard {});
    assert_ne!(AppRoute::ForbiddenPage {}, AppRoute::Login {});
}

// ---------------------------------------------------------------------------
// Route guard logic — mirrors the nav-item visibility logic in MainLayout
//
// These tests call the layout module's `nav_item_allowed` function through
// the public `layouts::main_layout` path.  Because the function is `pub(crate)`,
// tests in the same crate can access it under `#[cfg(test)]`.
// ---------------------------------------------------------------------------

use crate::layouts::main_layout::nav_item_allowed;

#[test]
fn admin_can_see_admin_settings_route() {
    assert!(
        nav_item_allowed(&[Role::Admin], &AppRoute::AdminSettings {}),
        "Admin must be allowed to see AdminSettings route"
    );
}

#[test]
fn viewer_cannot_see_admin_settings_route() {
    assert!(
        !nav_item_allowed(&[Role::Viewer], &AppRoute::AdminSettings {}),
        "Viewer must NOT be allowed to see AdminSettings route"
    );
}

#[test]
fn admin_can_see_all_routes() {
    let admin = vec![Role::Admin];
    let routes = [
        AppRoute::Dashboard {},
        AppRoute::JournalList {},
        AppRoute::ResourceList {},
        AppRoute::CourseList {},
        AppRoute::Imports {},
        AppRoute::CheckIn {},
        AppRoute::Reports {},
        AppRoute::Metrics {},
        AppRoute::AuditLogs {},
        AppRoute::AdminSettings {},
        AppRoute::RetentionSettings {},
    ];
    for route in &routes {
        assert!(
            nav_item_allowed(&admin, route),
            "Admin must be allowed to see route: {route:?}"
        );
    }
}

#[test]
fn viewer_sees_only_read_only_content_routes() {
    let viewer = vec![Role::Viewer];

    assert!(nav_item_allowed(&viewer, &AppRoute::Dashboard {}));
    assert!(nav_item_allowed(&viewer, &AppRoute::JournalList {}));
    assert!(nav_item_allowed(&viewer, &AppRoute::ResourceList {}));
    assert!(nav_item_allowed(&viewer, &AppRoute::CourseList {}));

    assert!(!nav_item_allowed(&viewer, &AppRoute::Imports {}));
    assert!(!nav_item_allowed(&viewer, &AppRoute::CheckIn {}));
    assert!(!nav_item_allowed(&viewer, &AppRoute::Reports {}));
    assert!(!nav_item_allowed(&viewer, &AppRoute::Metrics {}));
    assert!(!nav_item_allowed(&viewer, &AppRoute::AuditLogs {}));
    assert!(!nav_item_allowed(&viewer, &AppRoute::AdminSettings {}));
    assert!(!nav_item_allowed(&viewer, &AppRoute::RetentionSettings {}));
}

#[test]
fn auditor_can_see_audit_logs_but_not_admin_routes() {
    let auditor = vec![Role::Auditor];
    assert!(nav_item_allowed(&auditor, &AppRoute::AuditLogs {}));
    assert!(nav_item_allowed(&auditor, &AppRoute::Reports {}));
    assert!(!nav_item_allowed(&auditor, &AppRoute::AdminSettings {}));
    assert!(!nav_item_allowed(&auditor, &AppRoute::RetentionSettings {}));
    assert!(!nav_item_allowed(&auditor, &AppRoute::Imports {}));
}

#[test]
fn instructor_can_checkin_but_not_import() {
    let instructor = vec![Role::Instructor];
    assert!(nav_item_allowed(&instructor, &AppRoute::CheckIn {}));
    assert!(!nav_item_allowed(&instructor, &AppRoute::Imports {}));
    assert!(!nav_item_allowed(&instructor, &AppRoute::AuditLogs {}));
}
