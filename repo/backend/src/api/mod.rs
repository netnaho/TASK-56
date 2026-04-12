//! API route modules for the Scholarly backend.
//!
//! All modules in this file are fully implemented.
//! Phase progression (for history): 1→auth skeleton, 2→users/audit/config,
//! 3→journals/resources/attachments, 4→courses/sections, 5→checkins/metrics/
//! dashboards, 6→reports/retention, 7→health/roles/user-CRUD completion.

pub mod catchers;
pub mod download;
pub mod guards;

// Auth and identity ────────────────────────────────────────────────────────
pub mod auth;
pub mod users;

// Administration ───────────────────────────────────────────────────────────
pub mod admin_config;
pub mod artifact_backfill;
pub mod audit_logs;
pub mod health;
pub mod roles;
pub mod retention;

// Library content ──────────────────────────────────────────────────────────
pub mod attachments;
pub mod journals;
pub mod teaching_resources;

// Academic catalog ─────────────────────────────────────────────────────────
pub mod courses;
pub mod sections;

// Engagement & analytics ───────────────────────────────────────────────────
pub mod checkins;
pub mod dashboards;
pub mod metrics;
pub mod reports;
