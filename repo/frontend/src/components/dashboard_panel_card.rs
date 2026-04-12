//! Reusable dashboard panel card — renders a titled card with a list
//! of horizontal bars whose width is proportional to the row value.
//!
//! Every bar label is interpolated as a plain text node — we never use
//! `dangerous_inner_html` or any raw-HTML escape hatch, so the
//! server-supplied labels are always rendered safely.

use dioxus::prelude::*;

use crate::api::client::ApiError;
use crate::api::dashboards::DashboardPanel;

/// Visual state wrapper for a [`DashboardPanelCard`]. Each card owns
/// its own loading/error/data slot so the seven panels can fail
/// independently.
#[derive(Debug, Clone, PartialEq)]
pub enum PanelState {
    Loading,
    Loaded(DashboardPanel),
    Error(String),
}

impl PanelState {
    pub fn from_result(res: Result<DashboardPanel, ApiError>) -> Self {
        match res {
            Ok(panel) => PanelState::Loaded(panel),
            Err(err) => PanelState::Error(err.message),
        }
    }
}

#[derive(Props, Clone, PartialEq)]
pub struct DashboardPanelCardProps {
    pub title: String,
    pub state: PanelState,
}

/// Converts a `snake_case` metric key into a human title, e.g.
/// `"course_popularity"` → `"Course Popularity"`.
pub fn prettify_key(key: &str) -> String {
    key.split('_')
        .filter(|w| !w.is_empty())
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(c) => c.to_uppercase().chain(chars).collect::<String>(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[component]
pub fn DashboardPanelCard(props: DashboardPanelCardProps) -> Element {
    let state = props.state.clone();
    let title = props.title.clone();

    rsx! {
        article { class: "dashboard-card",
            header { class: "dashboard-card__header",
                h3 { "{title}" }
            }
            if let PanelState::Loading = &state {
                p { class: "text-muted", "Loading..." }
            }
            if let PanelState::Error(msg) = &state {
                div { class: "error-banner", "{msg}" }
            }
            if let PanelState::Loaded(panel) = &state {
                PanelBody { panel: panel.clone() }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct PanelBodyProps {
    panel: DashboardPanel,
}

#[component]
fn PanelBody(props: PanelBodyProps) -> Element {
    let panel = props.panel.clone();
    let rows = panel.rows.clone();
    let notes = panel.notes.clone();

    if rows.is_empty() {
        return rsx! {
            p { class: "text-muted", "No data in the selected window." }
            if !notes.is_empty() {
                ul { class: "dashboard-card__notes",
                    for note in notes.iter().cloned() {
                        li { class: "text-muted", em { "{note}" } }
                    }
                }
            }
        };
    }

    // Compute the max absolute value to scale bar widths. Guard against
    // zero / negative max so the division below is always safe.
    let mut max = 0.0f64;
    for r in rows.iter() {
        if r.value.abs() > max {
            max = r.value.abs();
        }
    }
    if max <= 0.0 {
        max = 1.0;
    }

    rsx! {
        ul { class: "dashboard-card__rows",
            for row in rows.iter().cloned() {
                {
                    let percent = ((row.value.abs() / max) * 100.0).clamp(0.0, 100.0);
                    let bar_style = format!("width: {:.1}%", percent);
                    let label = row.label.clone();
                    let value_str = format_value(row.value);
                    rsx! {
                        li { class: "bar-row",
                            div { class: "bar-row__label", "{label}" }
                            div { class: "bar-row__track",
                                div { class: "bar", style: "{bar_style}" }
                            }
                            div { class: "bar-row__value", "{value_str}" }
                        }
                    }
                }
            }
        }
        if !notes.is_empty() {
            ul { class: "dashboard-card__notes",
                for note in notes.iter().cloned() {
                    li { class: "text-muted", em { "{note}" } }
                }
            }
        }
    }
}

fn format_value(v: f64) -> String {
    if v == v.trunc() && v.abs() < 1e12 {
        format!("{:.0}", v)
    } else {
        format!("{:.2}", v)
    }
}
