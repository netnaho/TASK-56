//! Check-in repository — persistence for check-in records.
//!
//! Unlike the other domain areas (journals, courses, reports, etc.) which
//! delegate all SQL to dedicated repository modules, the check-in domain
//! keeps its persistence logic inline inside
//! [`crate::application::checkin_service`].
//!
//! **Reason:** check-in writes require tight coupling between the duplicate-
//! detection query, the insert, and the retry logic.  Extracting them into a
//! separate repository layer would split a single atomic read–modify–write
//! sequence across two files with no architectural benefit.  All check-in SQL
//! is therefore in the service module, following the same `sqlx::query`
//! parameterised pattern used everywhere else.
//!
//! This module is retained as a placeholder so the repository namespace
//! remains consistent and so that a future refactor can move the SQL here
//! without changing any public API.
