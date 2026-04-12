# Unit Tests

Unit tests for the backend live in `backend/src/` and are co-located with their
respective modules using `#[cfg(test)]` blocks. This is the standard Rust
convention and keeps tests close to the code they exercise.

This directory holds cross-cutting unit test utilities and reference files that
are shared across modules or that do not belong to a single backend source file.
