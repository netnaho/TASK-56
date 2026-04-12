//! Admin settings page — system-wide key/value configuration.
//!
//! Displays all `admin_settings` rows loaded from `GET /api/v1/admin/config`.
//! Each row can be edited inline; the save button calls
//! `PUT /api/v1/admin/config/<key>`.
//!
//! Accessible only to Admin users (enforced on the backend; the frontend
//! simply routes to `/admin/settings` which is gated by `required_role: Admin`
//! in the navigation definition).

use dioxus::prelude::*;
use serde_json::Value;

use crate::api::admin_config::{self, UpdateSettingInput};
use crate::state::AuthState;

#[component]
pub fn AdminSettings() -> Element {
    let auth = use_context::<Signal<AuthState>>();

    // ── State ────────────────────────────────────────────────────────────────
    let mut settings: Signal<Vec<admin_config::AdminSetting>> = use_signal(Vec::new);
    let mut loading = use_signal(|| true);
    let mut error: Signal<Option<String>> = use_signal(|| None);
    let mut save_status: Signal<Option<String>> = use_signal(|| None);

    // Per-row edit buffer: key → current input string.
    let mut edit_values: Signal<std::collections::HashMap<String, String>> =
        use_signal(std::collections::HashMap::new);

    // ── Load settings on mount ───────────────────────────────────────────────
    let token = auth.read().token.clone().unwrap_or_default();
    {
        let token = token.clone();
        use_effect(move || {
            let token = token.clone();
            spawn(async move {
                match admin_config::list_settings(&token).await {
                    Ok(data) => {
                        // Initialise the edit buffer with each setting's current
                        // value serialised to a string.
                        let mut buf = std::collections::HashMap::new();
                        for s in &data {
                            buf.insert(s.key.clone(), value_to_display(&s.value));
                        }
                        settings.set(data);
                        edit_values.set(buf);
                    }
                    Err(e) => {
                        error.set(Some(format!("Failed to load settings: {}", e.message)));
                    }
                }
                loading.set(false);
            });
        });
    }

    // ── Save handler ─────────────────────────────────────────────────────────
    let handle_save = move |key: String| {
        let token = auth.read().token.clone().unwrap_or_default();
        let raw = edit_values.read().get(&key).cloned().unwrap_or_default();
        spawn(async move {
            // Parse the edited string as JSON; fall back to a JSON string
            // literal if it doesn't parse as a raw value.
            let value: Value = serde_json::from_str(&raw).unwrap_or_else(|_| Value::String(raw));
            let input = UpdateSettingInput { value, description: None };
            match admin_config::update_setting(&token, &key, &input).await {
                Ok(updated) => {
                    settings.with_mut(|list| {
                        if let Some(s) = list.iter_mut().find(|s| s.key == key) {
                            *s = updated;
                        }
                    });
                    save_status.set(Some(format!("'{}' saved.", key)));
                }
                Err(e) => {
                    save_status.set(Some(format!("Save failed: {}", e.message)));
                }
            }
        });
    };

    rsx! {
        div { class: "page",

            // ── Header ───────────────────────────────────────────────────────
            div { class: "page-header",
                h1 { "Admin Settings" }
                p { class: "page-subtitle",
                    "System-wide configuration. Changes take effect immediately."
                }
            }

            // ── Status banner ────────────────────────────────────────────────
            if let Some(msg) = save_status.read().as_deref() {
                div { class: if msg.starts_with("Save failed") { "banner banner--error" } else { "banner banner--success" },
                    "{msg}"
                    button {
                        class: "banner__close",
                        onclick: move |_| save_status.set(None),
                        "×"
                    }
                }
            }

            // ── Error state ──────────────────────────────────────────────────
            if let Some(err) = error.read().as_deref() {
                div { class: "error-message",
                    "⚠ {err}"
                }
            }

            // ── Loading state ────────────────────────────────────────────────
            else if *loading.read() {
                div { class: "loading-spinner", "Loading settings…" }
            }

            // ── Empty state ──────────────────────────────────────────────────
            else if settings.read().is_empty() {
                div { class: "empty-state",
                    p { "No settings found." }
                }
            }

            // ── Settings table ───────────────────────────────────────────────
            else {
                div { class: "settings-table-wrapper",
                    table { class: "data-table",
                        thead {
                            tr {
                                th { "Key" }
                                th { "Value" }
                                th { "Description" }
                                th { "Last updated" }
                                th { "Action" }
                            }
                        }
                        tbody {
                            for setting in settings.read().iter() {
                                {
                                    let key = setting.key.clone();
                                    let key2 = key.clone();
                                    let key3 = key.clone();
                                    let description = setting.description.clone().unwrap_or_default();
                                    let updated_at = setting.updated_at.clone();
                                    rsx! {
                                        tr { key: "{key3}",
                                            td { class: "settings-table__key",
                                                code { "{key}" }
                                            }
                                            td { class: "settings-table__value",
                                                input {
                                                    class: "settings-input",
                                                    r#type: "text",
                                                    value: edit_values.read().get(&key).cloned().unwrap_or_default(),
                                                    oninput: move |e| {
                                                        edit_values.with_mut(|m| {
                                                            m.insert(key.clone(), e.value());
                                                        });
                                                    },
                                                }
                                            }
                                            td { class: "settings-table__desc",
                                                "{description}"
                                            }
                                            td { class: "settings-table__updated",
                                                "{updated_at}"
                                            }
                                            td {
                                                button {
                                                    class: "btn btn--primary btn--sm",
                                                    r#type: "button",
                                                    onclick: move |_| handle_save(key2.clone()),
                                                    "Save"
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
    }
}

/// Serialise a JSON value to a human-readable edit string.
///
/// Strings are returned without surrounding quotes; all other values are
/// rendered as compact JSON so they can be round-tripped through the input.
fn value_to_display(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        other => serde_json::to_string(other).unwrap_or_default(),
    }
}
