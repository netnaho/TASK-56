//! DataTable component — generic sortable, paginated data table.

use dioxus::prelude::*;

/// A reusable data table with sorting, pagination, and row selection.
///
/// TODO: Accept column definitions and row data generically. Implement
/// sort indicators, page controls, and optional row checkboxes.
#[component]
pub fn DataTable() -> Element {
    rsx! {
        div { class: "card",
            p { class: "stub-notice", "DataTable placeholder — implementation pending." }
        }
    }
}
