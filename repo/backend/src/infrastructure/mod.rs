//! Infrastructure layer — database pool, file storage, and bootstrap.
//!
//! Repositories from Phase 1 remain as stubs pending their respective
//! phases. The security-critical loaders live directly in
//! [`crate::application::auth_service`] for locality and will be migrated
//! into `repositories::user_repo` during Phase 3.

pub mod bootstrap;
pub mod database;
pub mod repositories;
pub mod scheduler;
pub mod storage;
