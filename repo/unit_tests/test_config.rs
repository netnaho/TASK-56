// Cross-cutting unit test reference for AppConfig.
//
// AppConfig is fully tested inline in backend/src/config/mod.rs using
// standard Rust `#[cfg(test)]` blocks. Those tests run as part of
// `cargo test --lib` from the backend crate.
//
// To run them:
//   cd backend && cargo test config
//
// Covered:
//   - test_default_config_loads: verifies from_env() produces non-empty URLs
//   - test_reports_path_defaults_under_attachments: verifies REPORTS_STORAGE_PATH
//     defaults to a sub-path of ATTACHMENT_STORAGE_PATH
//
// This file is intentionally not a Rust source file included in any crate;
// it serves as cross-cutting documentation. See unit_tests/README.md.

// No executable Rust test code lives here. See backend/src/config/mod.rs.
