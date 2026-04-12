/// Repository implementations — MySQL-backed persistence for each domain aggregate.
///
/// Each submodule implements the repository trait (or interface) for a
/// specific domain entity, translating between domain types and database
/// rows via sqlx queries.

pub mod user_repo;
pub mod role_repo;
pub mod journal_repo;
pub mod resource_repo;
pub mod course_repo;
pub mod section_repo;
pub mod checkin_repo;
pub mod metric_repo;
pub mod dashboard_repo;
pub mod report_repo;
pub mod audit_repo;
pub mod retention_repo;
pub mod attachment_repo;
