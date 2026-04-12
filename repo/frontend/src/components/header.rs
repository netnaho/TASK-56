//! Top-bar header — rendered inline by `layouts::main_layout::MainLayout`.
//!
//! The top bar (user display name, role badge, logout button) is part of
//! `MainLayout` rather than a standalone component, because it needs access
//! to the same auth context signal and navigator that the layout uses.
//! This module is retained as a placeholder for any future standalone header
//! widget needs.
