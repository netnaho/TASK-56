//! Explicit authorization layer.
//!
//! Three layers cooperate:
//!
//! 1. **Route-level** — Rocket request guards (`Principal`, `AdminOnly`, ...)
//!    reject unauthenticated/unprivileged callers before any handler body runs.
//! 2. **Function-level** — every sensitive service call requires a `&Principal`
//!    and invokes a `require_*` helper from this module as its first step.
//! 3. **Object-level** — helpers in [`scope`](crate::application::scope)
//!    constrain which rows a caller can see or mutate (department scope,
//!    ownership, etc.).
//!
//! This module hosts layers 1 & 2. It also defines the canonical capability
//! matrix (`capabilities`) that the frontend and docs read from.

use super::principal::{Principal, Role};
use crate::errors::{AppError, AppResult};

/// Named capabilities tracked by the RBAC matrix.
///
/// These are coarse-grained — routes and service methods check capabilities,
/// not individual permission strings. The matrix in [`role_allows`] is the
/// single source of truth.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Capability {
    // ---- Library content ----
    JournalRead,
    JournalWrite,
    JournalApprove,
    JournalPublish,
    ResourceRead,
    ResourceWrite,
    ResourceApprove,
    ResourcePublish,

    // ---- Attachments ----
    AttachmentRead,
    AttachmentWrite,
    AttachmentDelete,

    // ---- Course / section ----
    CourseRead,
    CourseWrite,
    CourseApprove,
    CoursePublish,
    SectionRead,
    SectionWrite,
    SectionApprove,
    SectionPublish,

    // ---- Bulk import / export ----
    ImportCourses,
    ImportSections,
    ExportCourses,
    ExportSections,

    // ---- Import / Export ----
    ImportExport,

    // ---- Check-in ----
    CheckinRead,
    CheckinWrite,
    CheckinAdmin,

    // ---- Metrics semantic layer ----
    MetricRead,
    MetricWrite,
    MetricApprove,

    // ---- Dashboards / Reports ----
    DashboardRead,
    DashboardWrite,
    DashboardViewSensitive,
    ReportRead,
    ReportExecute,
    ReportManage,

    // ---- Audit ----
    AuditRead,
    AuditExport,

    // ---- Administration ----
    AdminConfigRead,
    AdminConfigWrite,
    UserManage,
    RoleManage,
    RetentionManage,
}

/// The canonical capability matrix. Exactly one place, readable by docs.
///
/// | Capability                | Admin | Auditor | Librarian | DeptHead | Instructor | Viewer |
/// |---------------------------|:-----:|:-------:|:---------:|:--------:|:----------:|:------:|
/// | JournalRead               |   Y   |    .    |     Y     |    Y     |     Y      |   Y    |
/// | JournalWrite              |   Y   |    .    |     Y     |    .     |     .      |   .    |
/// | ResourceRead              |   Y   |    .    |     Y     |    Y     |     Y      |   Y    |
/// | ResourceWrite             |   Y   |    .    |     Y     |    .     |     Y*     |   .    |
/// | CourseRead                |   Y   |    .    |     Y     |    Y     |     Y      |   Y    |
/// | CourseWrite               |   Y   |    .    |     .     |    Y     |     .      |   .    |
/// | SectionRead               |   Y   |    .    |     Y     |    Y     |     Y      |   Y    |
/// | SectionWrite              |   Y   |    .    |     .     |    Y     |     Y**    |   .    |
/// | ImportExport              |   Y   |    .    |     Y     |    Y     |     .      |   .    |
/// | CheckinRead               |   Y   |    .    |     .     |    Y     |     Y      |   .    |
/// | CheckinWrite              |   Y   |    .    |     .     |    .     |     Y      |   .    |
/// | DashboardRead             |   Y   |    .    |     Y     |    Y     |     Y      |   Y    |
/// | DashboardWrite            |   Y   |    .    |     .     |    Y     |     .      |   .    |
/// | ReportRead                |   Y   |    Y    |     Y     |    Y     |     Y      |   .    |
/// | ReportExecute             |   Y   |    .    |     Y     |    Y     |     .      |   .    |
/// | ReportManage              |   Y   |    .    |     .     |    Y     |     .      |   .    |
/// | AuditRead                 |   Y   |    Y    |     .     |    .     |     .      |   .    |
/// | AuditExport               |   Y   |    .    |     .     |    .     |     .      |   .    |
/// | AdminConfigRead           |   Y   |    .    |     .     |    .     |     .      |   .    |
/// | AdminConfigWrite          |   Y   |    .    |     .     |    .     |     .      |   .    |
/// | UserManage                |   Y   |    .    |     .     |    .     |     .      |   .    |
/// | RoleManage                |   Y   |    .    |     .     |    .     |     .      |   .    |
/// | RetentionManage           |   Y   |    .    |     .     |    .     |     .      |   .    |
///
/// \* instructors can write resources they own
/// \*\* instructors can edit sections they teach
///
/// Object-level scope (ownership, department) is enforced separately via
/// `scope::filter_*` helpers at the repository boundary.
pub fn role_allows(role: Role, cap: Capability) -> bool {
    use Capability::*;
    use Role::*;

    match (role, cap) {
        // Admin — full access, modulo any future "hard" deny policies.
        // Includes `DashboardViewSensitive` and `MetricApprove` which
        // are strictly admin in Phase 5.
        (Admin, _) => true,

        // Auditor — cross-cutting compliance role. Read-only access to audit
        // logs and reports. No write, admin config, or user-management
        // capabilities (least-privilege baseline).
        (Auditor, AuditRead) => true,
        (Auditor, ReportRead) => true,

        // Librarian — owns the library workflow: write + approve + publish
        // for both journals and resources, and manages attachments.
        (Librarian, JournalRead | JournalWrite | JournalApprove | JournalPublish) => true,
        (Librarian, ResourceRead | ResourceWrite | ResourceApprove | ResourcePublish) => true,
        (Librarian, AttachmentRead | AttachmentWrite | AttachmentDelete) => true,
        (Librarian, CourseRead | SectionRead) => true,
        (Librarian, ExportCourses | ExportSections) => true,
        (Librarian, ImportExport) => true,
        (Librarian, MetricRead) => true,
        (Librarian, DashboardRead | ReportRead | ReportExecute) => true,

        // Department Head — the "Academic Scheduler" role in Phase 4.
        // Owns the course catalog for their department end-to-end: CRUD,
        // approval, publication, and bulk import/export.
        (DepartmentHead, JournalRead | ResourceRead) => true,
        (DepartmentHead, AttachmentRead) => true,
        (DepartmentHead, CourseRead | CourseWrite | CourseApprove | CoursePublish) => true,
        (DepartmentHead, SectionRead | SectionWrite | SectionApprove | SectionPublish) => true,
        (DepartmentHead, ImportCourses | ImportSections) => true,
        (DepartmentHead, ExportCourses | ExportSections) => true,
        (DepartmentHead, ImportExport) => true,
        (DepartmentHead, CheckinRead) => true,
        (DepartmentHead, MetricRead | MetricWrite) => true,
        (DepartmentHead, DashboardRead | DashboardWrite | DashboardViewSensitive) => true,
        (DepartmentHead, ReportRead | ReportExecute | ReportManage) => true,

        // Instructor — can draft resources they own but can't publish.
        // For courses, instructors read the catalog and maintain drafts
        // for sections they teach; approvals/publishing stay with the
        // Academic Scheduler role.
        (Instructor, JournalRead) => true,
        (Instructor, ResourceRead | ResourceWrite) => true,
        (Instructor, AttachmentRead | AttachmentWrite) => true,
        (Instructor, CourseRead) => true,
        (Instructor, SectionRead | SectionWrite) => true,
        (Instructor, ExportCourses | ExportSections) => true,
        (Instructor, CheckinRead | CheckinWrite) => true,
        (Instructor, MetricRead) => true,
        // Instructors see the dashboard tiles but not the sensitive
        // student-identifier view; masking kicks in at the service layer.
        (Instructor, DashboardRead | ReportRead) => true,

        // Viewer — strictly read access to published baselines.
        (Viewer, JournalRead | ResourceRead | CourseRead | SectionRead) => true,
        (Viewer, AttachmentRead) => true,
        (Viewer, DashboardRead) => true,

        // Default deny.
        _ => false,
    }
}

/// True if the principal holds *any* role that grants `cap`.
pub fn principal_can(principal: &Principal, cap: Capability) -> bool {
    principal.roles.iter().any(|r| role_allows(*r, cap))
}

/// Return `Ok(())` if the principal has the capability, else `Forbidden`.
/// This is the one-line helper that service methods call as their first step.
pub fn require(principal: &Principal, cap: Capability) -> AppResult<()> {
    if principal_can(principal, cap) {
        Ok(())
    } else {
        Err(AppError::Forbidden)
    }
}

/// Admin-only gate for admin config / user management / retention.
pub fn require_admin(principal: &Principal) -> AppResult<()> {
    if principal.is_admin() {
        Ok(())
    } else {
        Err(AppError::Forbidden)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn principal_with(roles: Vec<Role>) -> Principal {
        Principal {
            user_id: Uuid::new_v4(),
            session_id: Uuid::new_v4(),
            email: "t@t".into(),
            display_name: "Tester".into(),
            roles,
            department_id: None,
        }
    }

    #[test]
    fn admin_has_every_capability() {
        let p = principal_with(vec![Role::Admin]);
        for cap in [
            Capability::AuditRead,
            Capability::AdminConfigWrite,
            Capability::RetentionManage,
            Capability::UserManage,
            Capability::JournalWrite,
            Capability::ReportManage,
        ] {
            assert!(require(&p, cap).is_ok(), "admin should have {:?}", cap);
        }
    }

    #[test]
    fn viewer_cannot_write_anything() {
        let p = principal_with(vec![Role::Viewer]);
        assert!(require(&p, Capability::JournalWrite).is_err());
        assert!(require(&p, Capability::CourseWrite).is_err());
        assert!(require(&p, Capability::AuditRead).is_err());
        assert!(require(&p, Capability::AdminConfigRead).is_err());
        // But can read:
        assert!(require(&p, Capability::JournalRead).is_ok());
        assert!(require(&p, Capability::CourseRead).is_ok());
    }

    #[test]
    fn librarian_can_manage_library_but_not_audit() {
        let p = principal_with(vec![Role::Librarian]);
        assert!(require(&p, Capability::JournalWrite).is_ok());
        assert!(require(&p, Capability::ResourceWrite).is_ok());
        assert!(require(&p, Capability::AuditRead).is_err());
        assert!(require(&p, Capability::AdminConfigRead).is_err());
    }

    #[test]
    fn instructor_can_write_checkins_not_journals() {
        let p = principal_with(vec![Role::Instructor]);
        assert!(require(&p, Capability::CheckinWrite).is_ok());
        assert!(require(&p, Capability::ResourceWrite).is_ok());
        assert!(require(&p, Capability::JournalWrite).is_err());
    }

    #[test]
    fn require_admin_rejects_non_admins() {
        assert!(require_admin(&principal_with(vec![Role::Librarian])).is_err());
        assert!(require_admin(&principal_with(vec![Role::DepartmentHead])).is_err());
        assert!(require_admin(&principal_with(vec![Role::Admin])).is_ok());
    }

    #[test]
    fn composite_roles_union_capabilities() {
        // A user holding both Instructor and Librarian gets the union.
        let p = principal_with(vec![Role::Instructor, Role::Librarian]);
        assert!(require(&p, Capability::JournalWrite).is_ok()); // from Librarian
        assert!(require(&p, Capability::CheckinWrite).is_ok()); // from Instructor
        assert!(require(&p, Capability::AuditRead).is_err());
    }

    #[test]
    fn librarian_can_approve_and_publish_library_content() {
        let p = principal_with(vec![Role::Librarian]);
        assert!(require(&p, Capability::JournalApprove).is_ok());
        assert!(require(&p, Capability::JournalPublish).is_ok());
        assert!(require(&p, Capability::ResourceApprove).is_ok());
        assert!(require(&p, Capability::ResourcePublish).is_ok());
        // Attachment CRUD is part of the library workflow.
        assert!(require(&p, Capability::AttachmentWrite).is_ok());
        assert!(require(&p, Capability::AttachmentDelete).is_ok());
    }

    #[test]
    fn instructor_can_draft_resource_but_not_approve_or_publish() {
        let p = principal_with(vec![Role::Instructor]);
        assert!(require(&p, Capability::ResourceWrite).is_ok());
        assert!(require(&p, Capability::AttachmentWrite).is_ok());
        assert!(require(&p, Capability::ResourceApprove).is_err());
        assert!(require(&p, Capability::ResourcePublish).is_err());
        assert!(require(&p, Capability::JournalWrite).is_err());
        assert!(require(&p, Capability::AttachmentDelete).is_err());
    }

    #[test]
    fn viewer_reads_attachments_but_cannot_write_or_delete() {
        let p = principal_with(vec![Role::Viewer]);
        assert!(require(&p, Capability::AttachmentRead).is_ok());
        assert!(require(&p, Capability::AttachmentWrite).is_err());
        assert!(require(&p, Capability::AttachmentDelete).is_err());
    }

    #[test]
    fn auditor_can_read_audit_and_reports_nothing_else() {
        let p = principal_with(vec![Role::Auditor]);
        // Granted capabilities.
        assert!(require(&p, Capability::AuditRead).is_ok());
        assert!(require(&p, Capability::ReportRead).is_ok());
        // Explicitly denied: no export, write, admin, or user management.
        assert!(require(&p, Capability::AuditExport).is_err());
        assert!(require(&p, Capability::ReportExecute).is_err());
        assert!(require(&p, Capability::ReportManage).is_err());
        assert!(require(&p, Capability::AdminConfigRead).is_err());
        assert!(require(&p, Capability::AdminConfigWrite).is_err());
        assert!(require(&p, Capability::UserManage).is_err());
        assert!(require(&p, Capability::RoleManage).is_err());
        assert!(require(&p, Capability::RetentionManage).is_err());
        assert!(require(&p, Capability::JournalWrite).is_err());
        assert!(require(&p, Capability::CourseWrite).is_err());
        assert!(require(&p, Capability::CheckinWrite).is_err());
    }

    #[test]
    fn auditor_and_admin_are_only_roles_with_audit_read() {
        assert!(role_allows(Role::Auditor, Capability::AuditRead));
        assert!(role_allows(Role::Admin, Capability::AuditRead));
        assert!(!role_allows(Role::Librarian, Capability::AuditRead));
        assert!(!role_allows(Role::DepartmentHead, Capability::AuditRead));
        assert!(!role_allows(Role::Instructor, Capability::AuditRead));
        assert!(!role_allows(Role::Viewer, Capability::AuditRead));
    }

    #[test]
    fn auditor_composite_with_librarian_unions_capabilities() {
        // User holding both Auditor and Librarian gets the union.
        let p = principal_with(vec![Role::Auditor, Role::Librarian]);
        assert!(require(&p, Capability::AuditRead).is_ok()); // from Auditor
        assert!(require(&p, Capability::JournalWrite).is_ok()); // from Librarian
        assert!(require(&p, Capability::AdminConfigRead).is_err()); // neither grants this
    }
}
