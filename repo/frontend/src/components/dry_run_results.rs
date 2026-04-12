//! Reusable `<DryRunResults>` component — renders the row-by-row
//! validation table for an [`ImportReport`]. Each row that accumulated
//! multiple field errors expands into one table row per error so every
//! issue is individually visible.

use dioxus::prelude::*;

use crate::api::imports::{FieldError, ImportReport, RowReport};

#[derive(Props, Clone, PartialEq)]
pub struct DryRunResultsProps {
    pub report: ImportReport,
}

#[component]
pub fn DryRunResults(props: DryRunResultsProps) -> Element {
    let report = props.report.clone();

    if report.rows.is_empty() {
        return rsx! {
            p { class: "text-muted", "No row-level details available." }
        };
    }

    // Flatten rows → one entry per field error, or a single entry for
    // rows that validated cleanly. This keeps the rsx! body simple and
    // sidesteps the restriction on nested rsx! expressions inside a
    // `for` loop.
    let flat_rows: Vec<FlatRow> = report
        .rows
        .iter()
        .flat_map(flatten_row)
        .collect();

    rsx! {
        div { class: "dry-run-table",
            div { class: "dry-run-row dry-run-row--head",
                div { "Row" }
                div { "Status" }
                div { "Field" }
                div { "Message" }
            }
            for entry in flat_rows.iter().cloned() {
                DryRunFlatRowView { key: "{entry.key}", entry: entry.clone() }
            }
        }
    }
}

/// A single display row in the flattened results table.
#[derive(Clone, PartialEq)]
struct FlatRow {
    key: String,
    row_index_label: String,
    show_status: bool,
    ok: bool,
    field: String,
    message: String,
}

fn flatten_row(row: &RowReport) -> Vec<FlatRow> {
    let status_ok = row.ok && row.errors.is_empty();
    if row.errors.is_empty() {
        vec![FlatRow {
            key: format!("row-{}-summary", row.row_index),
            row_index_label: row.row_index.to_string(),
            show_status: true,
            ok: status_ok,
            field: "-".to_string(),
            message: "-".to_string(),
        }]
    } else {
        row.errors
            .iter()
            .enumerate()
            .map(|(idx, err)| {
                let FieldError { field, message } = err.clone();
                FlatRow {
                    key: format!("row-{}-err-{}", row.row_index, idx),
                    row_index_label: if idx == 0 {
                        row.row_index.to_string()
                    } else {
                        String::new()
                    },
                    show_status: idx == 0,
                    ok: false,
                    field,
                    message,
                }
            })
            .collect()
    }
}

#[derive(Props, Clone, PartialEq)]
struct DryRunFlatRowProps {
    entry: FlatRow,
}

#[component]
fn DryRunFlatRowView(props: DryRunFlatRowProps) -> Element {
    let entry = props.entry.clone();
    let status_class = if entry.ok { "ok" } else { "error" };
    let status_label = if entry.ok { "OK" } else { "ERROR" };

    rsx! {
        div { class: "dry-run-row {status_class}",
            div { "{entry.row_index_label}" }
            div {
                if entry.show_status {
                    span { class: "dry-run-badge dry-run-badge--{status_class}",
                        "{status_label}"
                    }
                }
            }
            div { "{entry.field}" }
            div { "{entry.message}" }
        }
    }
}
