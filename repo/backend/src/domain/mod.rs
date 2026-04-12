/// Domain models — core business entities and value objects.
///
/// Each submodule defines the domain types for a bounded context.
/// Domain models are persistence-agnostic; mapping to/from database
/// rows happens in the infrastructure layer.

pub mod audit;
pub mod auth;
pub mod checkin;
pub mod course;
pub mod dashboard;
pub mod journal;
pub mod metric;
pub mod report;
pub mod retention;
pub mod role;
pub mod section;
pub mod teaching_resource;
pub mod user;
pub mod versioning;
