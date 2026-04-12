//! Scholarly Frontend — Route Definitions
//!
//! Declares every navigable route in the application using a `Routable`
//! enum. Dioxus-router derives the mapping from URL path to component
//! automatically.

use dioxus::prelude::*;
use dioxus_router::prelude::*;

use crate::layouts::main_layout::MainLayout;
use crate::pages::{
    admin_settings::AdminSettings,
    audit_logs::AuditLogs,
    checkin::CheckIn,
    course_detail::CourseDetail,
    course_list::CourseList,
    dashboard::Dashboard,
    forbidden::ForbiddenPage,
    imports::Imports,
    journal_detail::JournalDetail,
    journal_list::JournalList,
    login::Login,
    metrics::Metrics,
    reports::Reports,
    resource_detail::ResourceDetail,
    resource_list::ResourceList,
    retention_settings::RetentionSettings,
    section_detail::SectionDetail,
    section_list::SectionList,
};

/// Top-level route enum for the Scholarly application.
///
/// Each variant maps a URL pattern to a page component. Routes nested
/// under `#[layout(MainLayout)]` share the sidebar/header chrome.
#[derive(Clone, Routable, Debug, PartialEq)]
pub enum AppRoute {
    /// Public login page — rendered without the main layout chrome.
    #[route("/login")]
    Login {},

    /// 403 page — accessible without a session so authorization
    /// failures can redirect here cleanly.
    #[route("/forbidden")]
    ForbiddenPage {},

    /// All authenticated routes share the MainLayout wrapper.
    #[layout(MainLayout)]
        /// Dashboard / home page.
        #[route("/")]
        Dashboard {},

        /// Journal catalogue listing.
        #[route("/library/journals")]
        JournalList {},

        /// Detail view for a single journal (Phase 3).
        #[route("/library/journals/:id")]
        JournalDetail { id: String },

        /// General resource listing.
        #[route("/library/resources")]
        ResourceList {},

        /// Detail view for a single teaching resource (Phase 3).
        #[route("/library/resources/:id")]
        ResourceDetail { id: String },

        /// Course listing.
        #[route("/courses")]
        CourseList {},

        /// Detail view for a single course.
        #[route("/courses/:id")]
        CourseDetail { id: String },

        /// Sections within a course.
        #[route("/courses/:id/sections")]
        SectionList { id: String },

        /// Detail view for a single section.
        #[route("/sections/:id")]
        SectionDetail { id: String },

        /// Bulk import / export page.
        #[route("/imports")]
        Imports {},

        /// Resource check-in page.
        #[route("/checkin")]
        CheckIn {},

        /// Metric definitions catalog.
        #[route("/metrics")]
        Metrics {},

        /// Reports and analytics.
        #[route("/reports")]
        Reports {},

        /// Audit log viewer.
        #[route("/audit")]
        AuditLogs {},

        /// System-wide admin settings.
        #[route("/admin/settings")]
        AdminSettings {},

        /// Retention policy settings.
        #[route("/admin/retention")]
        RetentionSettings {},
    // end layout
}
