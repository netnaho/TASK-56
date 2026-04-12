//! SearchBar component — text input with debounced search callback.

use dioxus::prelude::*;

/// A search input field with optional placeholder and debounced change
/// notification.
///
/// TODO: Wire up debounce logic and emit search queries to the parent.
#[component]
pub fn SearchBar() -> Element {
    rsx! {
        input {
            class: "search-bar",
            r#type: "text",
            placeholder: "Search...",
            // TODO: oninput handler with debounce
        }
    }
}
