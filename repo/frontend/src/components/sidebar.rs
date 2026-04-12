//! Sidebar sub-components.
//!
//! The full sidebar navigation is rendered by `layouts::main_layout`.
//! This module provides the `SidebarSection` section-heading component used
//! to label logical groups of navigation items.

use dioxus::prelude::*;

/// A non-interactive section heading rendered above a group of navigation
/// links in the sidebar.
#[component]
pub fn SidebarSection(title: String) -> Element {
    rsx! {
        div { class: "sidebar__section-title", "{title}" }
    }
}
