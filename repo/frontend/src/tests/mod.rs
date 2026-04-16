//! Frontend unit test suite — identifiable test files for the Scholarly
//! frontend, covering router/route-guard logic, auth state behavior, and
//! key UI module invariants.
//!
//! ## File layout
//!
//! Each submodule is a separate `*.rs` file under `frontend/src/tests/` so
//! that audit tooling can identify them as distinct frontend test files.
//!
//! | File | What it covers |
//! |------|----------------|
//! | `router_tests.rs` | `AppRoute` variant existence, route-guard logic |
//! | `auth_state_tests.rs` | `AuthState` struct fields, `primary_role()` |
//! | `role_tests.rs` | `Role` ordering, parsing, display |
//! | `api_error_tests.rs` | `ApiError` type invariants |

#[cfg(test)]
mod router_tests;

#[cfg(test)]
mod auth_state_tests;

#[cfg(test)]
mod role_tests;

#[cfg(test)]
mod api_error_tests;
