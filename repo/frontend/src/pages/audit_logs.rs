//! Audit logs page — tamper-evident system event history.
//!
//! Loads audit entries from `GET /api/v1/audit-logs` with optional filters
//! (action prefix, date range). Exposes a "Verify Chain" button that calls
//! `GET /api/v1/audit-logs/verify-chain` and shows the integrity status.
//!
//! Actor emails and IP addresses may be `[ANONYMIZED]` or `[REDACTED]`
//! depending on whether retention anonymisation has run and the current
//! viewer's role.

use dioxus::prelude::*;

use crate::api::audit_logs::{self, AuditLogEntry, AuditLogQuery};
use crate::state::AuthState;

#[component]
pub fn AuditLogs() -> Element {
    let auth = use_context::<Signal<AuthState>>();

    // ── Filter state ─────────────────────────────────────────────────────────
    let mut filter_action: Signal<String> = use_signal(String::new);
    let mut filter_from: Signal<String> = use_signal(String::new);
    let mut filter_to: Signal<String> = use_signal(String::new);
    let mut limit: Signal<u32> = use_signal(|| 100u32);

    // ── Data state ───────────────────────────────────────────────────────────
    let mut entries: Signal<Vec<AuditLogEntry>> = use_signal(Vec::new);
    let mut total: Signal<usize> = use_signal(|| 0);
    let mut loading = use_signal(|| true);
    let mut error: Signal<Option<String>> = use_signal(|| None);

    // ── Chain verify state ───────────────────────────────────────────────────
    let mut chain_status: Signal<Option<audit_logs::ChainStatus>> = use_signal(|| None);
    let mut chain_loading = use_signal(|| false);
    let mut chain_error: Signal<Option<String>> = use_signal(|| None);

    // ── Initial load ─────────────────────────────────────────────────────────
    let token_for_load = auth.read().token.clone().unwrap_or_default();
    use_effect(move || {
        let token = token_for_load.clone();
        spawn(async move {
            fetch_entries(token, filter_action, filter_from, filter_to, limit, entries, total, error).await;
            loading.set(false);
        });
    });

    // ── Search handler ───────────────────────────────────────────────────────
    let auth_for_search = auth.clone();
    let handle_search = move |_: Event<MouseData>| {
        let token = auth_for_search.read().token.clone().unwrap_or_default();
        loading.set(true);
        error.set(None);
        spawn(async move {
            fetch_entries(token, filter_action, filter_from, filter_to, limit, entries, total, error).await;
            loading.set(false);
        });
    };
    let auth_for_keydown = auth.clone();
    let mut handle_search_keydown = move || {
        let token = auth_for_keydown.read().token.clone().unwrap_or_default();
        loading.set(true);
        error.set(None);
        spawn(async move {
            fetch_entries(token, filter_action, filter_from, filter_to, limit, entries, total, error).await;
            loading.set(false);
        });
    };

    // ── Verify chain handler ─────────────────────────────────────────────────
    let handle_verify = {
        let auth3 = auth.clone();
        move |_| {
            let token = auth3.read().token.clone().unwrap_or_default();
            chain_loading.set(true);
            chain_status.set(None);
            chain_error.set(None);
            spawn(async move {
                match audit_logs::verify_chain(&token).await {
                    Ok(status) => chain_status.set(Some(status)),
                    Err(e) => chain_error.set(Some(format!("Chain verify failed: {}", e.message))),
                }
                chain_loading.set(false);
            });
        }
    };

    rsx! {
        div { class: "page",

            // ── Header ───────────────────────────────────────────────────────
            div { class: "page-header",
                h1 { "Audit Logs" }
                p { class: "page-subtitle",
                    "Tamper-evident event log. Every entry is SHA-256 chained."
                }
            }

            // ── Chain status banner ──────────────────────────────────────────
            if let Some(ref status) = *chain_status.read() {
                div {
                    class: if status.valid { "banner banner--success" } else { "banner banner--error" },
                    if status.valid {
                        "✓ Chain intact — {status.total_entries} entries verified."
                    } else {
                        "✗ Chain broken at sequence #{status.broken_at_sequence:?} — {status.message}"
                    }
                }
            }
            if let Some(ref err) = *chain_error.read() {
                div { class: "banner banner--error", "{err}" }
            }

            // ── Filter row ───────────────────────────────────────────────────
            div { class: "filter-bar",
                input {
                    class: "filter-bar__input",
                    r#type: "text",
                    placeholder: "Filter by action (e.g. auth.login)",
                    value: "{filter_action}",
                    oninput: move |e| filter_action.set(e.value()),
                    onkeydown: move |e| {
                        if e.key() == Key::Enter {
                            handle_search_keydown();
                        }
                    },
                }
                input {
                    class: "filter-bar__input",
                    r#type: "datetime-local",
                    title: "From (UTC)",
                    value: "{filter_from}",
                    oninput: move |e| filter_from.set(e.value()),
                }
                input {
                    class: "filter-bar__input",
                    r#type: "datetime-local",
                    title: "To (UTC)",
                    value: "{filter_to}",
                    oninput: move |e| filter_to.set(e.value()),
                }
                select {
                    class: "filter-bar__select",
                    onchange: move |e| {
                        if let Ok(n) = e.value().parse::<u32>() {
                            limit.set(n);
                        }
                    },
                    option { value: "50",  "50 entries" }
                    option { value: "100", selected: true, "100 entries" }
                    option { value: "250", "250 entries" }
                    option { value: "500", "500 entries" }
                }
                button {
                    class: "btn btn--primary",
                    r#type: "button",
                    onclick: handle_search,
                    "Search"
                }
                button {
                    class: "btn btn--secondary",
                    r#type: "button",
                    disabled: *chain_loading.read(),
                    onclick: handle_verify,
                    if *chain_loading.read() { "Verifying…" } else { "Verify Chain" }
                }
            }

            // ── Error state ──────────────────────────────────────────────────
            if let Some(ref err) = *error.read() {
                div { class: "error-message", "⚠ {err}" }
            }

            // ── Loading state ────────────────────────────────────────────────
            else if *loading.read() {
                div { class: "loading-spinner", "Loading audit log…" }
            }

            // ── Empty state ──────────────────────────────────────────────────
            else if entries.read().is_empty() {
                div { class: "empty-state",
                    p { "No audit log entries match the current filters." }
                }
            }

            // ── Results ──────────────────────────────────────────────────────
            else {
                div { class: "table-meta",
                    "Showing {entries.read().len()} of {total} entries"
                }
                div { class: "table-wrapper",
                    table { class: "data-table data-table--audit",
                        thead {
                            tr {
                                th { "#" }
                                th { "Timestamp" }
                                th { "Actor" }
                                th { "Action" }
                                th { "Entity type" }
                                th { "IP" }
                            }
                        }
                        tbody {
                            for entry in entries.read().iter() {
                                {
                                    let seq = entry.sequence_number;
                                    let ts = entry.created_at.clone();
                                    let actor = entry.actor_email.clone().unwrap_or_else(|| "—".to_string());
                                    let action = entry.action.clone();
                                    let entity_type = entry.target_entity_type.clone().unwrap_or_else(|| "—".to_string());
                                    let ip = entry.ip_address.clone().unwrap_or_else(|| "—".to_string());
                                    rsx! {
                                        tr { key: "{seq}",
                                            td { class: "audit-seq", "{seq}" }
                                            td { class: "audit-ts", "{ts}" }
                                            td { class: "audit-actor", "{actor}" }
                                            td {
                                                class: "audit-action",
                                                span { class: "action-badge", "{action}" }
                                            }
                                            td { class: "audit-entity", "{entity_type}" }
                                            td { class: "audit-ip", "{ip}" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Fetch audit entries applying the current filter signals.
async fn fetch_entries(
    token: String,
    filter_action: Signal<String>,
    filter_from: Signal<String>,
    filter_to: Signal<String>,
    limit: Signal<u32>,
    mut entries: Signal<Vec<AuditLogEntry>>,
    mut total: Signal<usize>,
    mut error: Signal<Option<String>>,
) {
    let action_val = filter_action.read().clone();
    let from_val = filter_from.read().clone();
    let to_val = filter_to.read().clone();

    // Convert datetime-local values (YYYY-MM-DDTHH:MM) to RFC3339.
    let from_rfc = local_to_rfc3339(&from_val);
    let to_rfc = local_to_rfc3339(&to_val);

    let query = AuditLogQuery {
        action: if action_val.is_empty() { None } else { Some(action_val) },
        from: from_rfc,
        to: to_rfc,
        limit: *limit.read(),
        ..Default::default()
    };

    match audit_logs::list(&token, &query).await {
        Ok(envelope) => {
            let n = envelope.count;
            entries.set(envelope.entries);
            total.set(n);
            error.set(None);
        }
        Err(e) => {
            error.set(Some(e.message.clone()));
        }
    }
}

/// Convert a `datetime-local` input value (`YYYY-MM-DDTHH:MM`) to RFC3339
/// (`YYYY-MM-DDTHH:MM:00Z`) for the backend query parameter. Returns `None`
/// when the input is empty.
fn local_to_rfc3339(s: &str) -> Option<String> {
    if s.is_empty() {
        None
    } else if s.len() == 16 {
        // datetime-local produces "YYYY-MM-DDTHH:MM"; append seconds and Z.
        Some(format!("{}:00Z", s))
    } else {
        Some(s.to_string())
    }
}
