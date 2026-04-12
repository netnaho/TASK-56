//! Phase 5 check-in page — the one-tap section check-in workflow.
//!
//! Admins and instructors can check into one of their visible sections
//! with a single click. The page collects a minimal client-side device
//! fingerprint (browser-visible signals only — never SSID) and an
//! optional informational `network_hint` text field.
//!
//! Duplicate check-ins within the backend-configured window return a
//! 409 response that the UI surfaces as an amber retry panel. Users
//! pick a reason from the server-supplied list and resubmit against the
//! most recent non-duplicate original. Network-rule violations (403)
//! render as a red banner reminding the user that "on-campus" is
//! determined by the admin-configured IP/CIDR allowlist.

use dioxus::prelude::*;
use dioxus_router::prelude::*;
use serde_json::json;
use wasm_bindgen::{prelude::Closure, JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;

use crate::api::checkins::{
    self, CheckinInput, CheckinRetryInput, CheckinType, CheckinView, RetryReason,
};
use crate::api::sections::{self, SectionView};
use crate::router::AppRoute;
use crate::state::AuthState;
use crate::types::Role;

/// Async sleep helper — resolves a JS Promise via `setTimeout`, invoked
/// through `Reflect` so we don't depend on any additional `web-sys`
/// feature flags.
async fn sleep_ms(ms: i32) {
    let promise = js_sys::Promise::new(&mut |resolve, _reject| {
        let Some(window) = web_sys::window() else {
            return;
        };
        let window_js: JsValue = window.into();
        let set_timeout = match js_sys::Reflect::get(&window_js, &JsValue::from_str("setTimeout")) {
            Ok(v) => v,
            Err(_) => return,
        };
        let set_timeout: js_sys::Function = match set_timeout.dyn_into() {
            Ok(f) => f,
            Err(_) => return,
        };
        let cb = Closure::once_into_js(move || {
            let _ = resolve.call0(&JsValue::NULL);
        });
        let _ = set_timeout.call2(&window_js, &cb, &JsValue::from_f64(ms as f64));
    });
    let _ = JsFuture::from(promise).await;
}

/// Returns `true` when the caller may perform a check-in (write path).
/// Admins and instructors match the backend's `CheckinWrite` permission.
fn caller_can_checkin(auth: &AuthState) -> bool {
    auth.user
        .as_ref()
        .map(|u| {
            u.roles
                .iter()
                .any(|r| matches!(r, Role::Admin | Role::Instructor))
        })
        .unwrap_or(false)
}

/// Builds a minimal browser-visible device fingerprint for submission.
///
/// All values are read from public browser APIs via `js-sys::Reflect`
/// (so we don't need to enable extra `web-sys` features). There is
/// NO attempt to enumerate Wi-Fi networks, geolocation, or any other
/// sensor data — the fingerprint contains only harmless signals that
/// the browser hands out by default.
fn build_device_fingerprint() -> serde_json::Value {
    let window_js: JsValue = match web_sys::window() {
        Some(w) => w.into(),
        None => return json!({}),
    };

    let navigator = read_prop(&window_js, "navigator").unwrap_or(JsValue::NULL);
    let user_agent = read_prop(&navigator, "userAgent")
        .and_then(|v| v.as_string())
        .unwrap_or_default();
    let platform = read_prop(&navigator, "platform")
        .and_then(|v| v.as_string())
        .unwrap_or_default();
    let language = read_prop(&navigator, "language")
        .and_then(|v| v.as_string())
        .unwrap_or_default();

    let screen = read_prop(&window_js, "screen").unwrap_or(JsValue::NULL);
    let screen_w = read_prop(&screen, "width")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0) as i32;
    let screen_h = read_prop(&screen, "height")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0) as i32;

    let timezone = resolve_timezone().unwrap_or_default();

    json!({
        "user_agent": user_agent,
        "platform": platform,
        "language": language,
        "screen": { "w": screen_w, "h": screen_h },
        "timezone": timezone,
    })
}

/// Reads an arbitrary property off a JS object via `Reflect.get`.
fn read_prop(target: &JsValue, key: &str) -> Option<JsValue> {
    if target.is_null() || target.is_undefined() {
        return None;
    }
    js_sys::Reflect::get(target, &JsValue::from_str(key)).ok()
}

/// Calls `Intl.DateTimeFormat().resolvedOptions().timeZone` via js-sys.
fn resolve_timezone() -> Option<String> {
    let global = js_sys::global();
    let intl = js_sys::Reflect::get(&global, &JsValue::from_str("Intl")).ok()?;
    let dtf_ctor = js_sys::Reflect::get(&intl, &JsValue::from_str("DateTimeFormat")).ok()?;
    let dtf_ctor: js_sys::Function = dtf_ctor.dyn_into().ok()?;
    let instance = js_sys::Reflect::construct(&dtf_ctor, &js_sys::Array::new()).ok()?;
    let resolved_fn =
        js_sys::Reflect::get(&instance, &JsValue::from_str("resolvedOptions")).ok()?;
    let resolved_fn: js_sys::Function = resolved_fn.dyn_into().ok()?;
    let opts = resolved_fn.call0(&instance).ok()?;
    let tz = js_sys::Reflect::get(&opts, &JsValue::from_str("timeZone")).ok()?;
    tz.as_string()
}

/// Visual state for the primary result banner.
#[derive(Debug, Clone, PartialEq)]
enum ResultBanner {
    None,
    Success { message: String },
    Duplicate { window_minutes: i32 },
    NetworkBlocked { message: String },
    Error { message: String },
}

#[component]
pub fn CheckIn() -> Element {
    let mut auth = use_context::<Signal<AuthState>>();
    let navigator = use_navigator();

    let can_checkin = caller_can_checkin(&auth.read());

    // Section list + selection.
    let mut section_list = use_signal(Vec::<SectionView>::new);
    let mut sections_loading = use_signal(|| true);
    let mut sections_error = use_signal(|| Option::<String>::None);
    let mut selected_section_id = use_signal(String::new);

    // Optional network hint text box (informational only).
    let mut network_hint_input = use_signal(String::new);

    // Result banner + retry flow state.
    let mut banner = use_signal(|| ResultBanner::None);
    let mut retry_reasons = use_signal(Vec::<RetryReason>::new);
    let mut selected_retry_reason = use_signal(String::new);
    let mut show_retry_picker = use_signal(|| false);
    let mut submitting = use_signal(|| false);
    let mut cooldown_seconds = use_signal(|| 0u32);

    // History panel for the currently selected section.
    let mut history = use_signal(Vec::<CheckinView>::new);
    let mut history_error = use_signal(|| Option::<String>::None);
    let mut history_reload_tick = use_signal(|| 0u32);

    // ── Load sections on mount ──────────────────────────────────
    use_effect(move || {
        let token = match auth.read().token.clone() {
            Some(t) => t,
            None => return,
        };
        sections_loading.set(true);
        sections_error.set(None);
        spawn(async move {
            match sections::list(&token, None, 50, 0).await {
                Ok(rows) => {
                    if let Some(first) = rows.first() {
                        if selected_section_id.read().is_empty() {
                            selected_section_id.set(first.id.clone());
                        }
                    }
                    section_list.set(rows);
                    sections_loading.set(false);
                }
                Err(err) => {
                    sections_loading.set(false);
                    if err.is_unauthorized() {
                        auth.set(AuthState::default());
                        AuthState::clear_storage();
                        navigator.push(AppRoute::Login {});
                        return;
                    }
                    if err.is_forbidden() {
                        navigator.push(AppRoute::ForbiddenPage {});
                        return;
                    }
                    sections_error.set(Some(format!(
                        "Failed to load sections: {}",
                        err.message
                    )));
                }
            }
        });
    });

    // ── Load retry reasons on mount (best-effort) ───────────────
    use_effect(move || {
        let token = match auth.read().token.clone() {
            Some(t) => t,
            None => return,
        };
        spawn(async move {
            if let Ok(reasons) = checkins::list_retry_reasons(&token).await {
                if let Some(first) = reasons.first() {
                    if selected_retry_reason.read().is_empty() {
                        selected_retry_reason.set(first.reason_code.clone());
                    }
                }
                retry_reasons.set(reasons);
            }
        });
    });

    // ── Load history whenever the selected section changes ─────
    use_effect(move || {
        let _tick = history_reload_tick.read();
        let token = match auth.read().token.clone() {
            Some(t) => t,
            None => return,
        };
        let section_id = selected_section_id.read().clone();
        if section_id.is_empty() {
            return;
        }
        history_error.set(None);
        spawn(async move {
            match checkins::list_checkins(&token, &section_id).await {
                Ok(rows) => {
                    let mut limited = rows;
                    limited.truncate(20);
                    history.set(limited);
                }
                Err(err) => {
                    if err.is_unauthorized() {
                        auth.set(AuthState::default());
                        AuthState::clear_storage();
                        navigator.push(AppRoute::Login {});
                        return;
                    }
                    if !err.is_forbidden() {
                        history_error.set(Some(format!(
                            "Failed to load history: {}",
                            err.message
                        )));
                    }
                }
            }
        });
    });

    // ── Cooldown countdown helper ──────────────────────────────
    //
    // Called after a successful check-in / retry; caller must clone
    // the `cooldown_seconds` signal into the spawned task.
    fn spawn_cooldown(mut signal: Signal<u32>) {
        signal.set(10);
        spawn(async move {
            for _ in 0..10u32 {
                sleep_ms(1_000).await;
                let current = signal();
                if current == 0 {
                    break;
                }
                signal.set(current - 1);
            }
        });
    }

    // ── Primary check-in action ─────────────────────────────────
    let do_checkin = move |_evt: MouseEvent| {
        if submitting() || cooldown_seconds() > 0 {
            return;
        }
        let section_id = selected_section_id.read().clone();
        if section_id.is_empty() {
            banner.set(ResultBanner::Error {
                message: "Pick a section first.".to_string(),
            });
            return;
        }
        let token = match auth.read().token.clone() {
            Some(t) => t,
            None => return,
        };
        let fingerprint = build_device_fingerprint();
        let network_hint = {
            let s = network_hint_input();
            if s.trim().is_empty() { None } else { Some(s) }
        };
        let input = CheckinInput {
            section_id: section_id.clone(),
            checkin_type: CheckinType::Geofence,
            device_fingerprint: Some(fingerprint),
            network_hint,
        };
        submitting.set(true);
        banner.set(ResultBanner::None);
        show_retry_picker.set(false);
        spawn(async move {
            match checkins::check_in(&token, &input).await {
                Ok(result) => {
                    submitting.set(false);
                    banner.set(ResultBanner::Success {
                        message: format!("Checked in at {}", result.view.checked_in_at),
                    });
                    history_reload_tick.with_mut(|n| *n += 1);
                    spawn_cooldown(cooldown_seconds);
                }
                Err(err) => {
                    submitting.set(false);
                    if err.is_unauthorized() {
                        auth.set(AuthState::default());
                        AuthState::clear_storage();
                        navigator.push(AppRoute::Login {});
                        return;
                    }
                    // 409 duplicate → retry flow.
                    if err.status == 409 || err.code == "conflict" {
                        // The backend ships the duplicate window in the
                        // CheckinResult envelope on success, but on a
                        // 409 we only have the error message. Default
                        // to 0 and surface the raw message as context.
                        banner.set(ResultBanner::Duplicate {
                            window_minutes: 0,
                        });
                        show_retry_picker.set(true);
                        return;
                    }
                    // 403 network rule block.
                    if (err.status == 403 || err.code == "forbidden")
                        && err.message.to_lowercase().contains("network")
                    {
                        banner.set(ResultBanner::NetworkBlocked {
                            message: err.message,
                        });
                        return;
                    }
                    if err.is_forbidden() {
                        navigator.push(AppRoute::ForbiddenPage {});
                        return;
                    }
                    banner.set(ResultBanner::Error { message: err.message });
                }
            }
        });
    };

    // ── Retry action ────────────────────────────────────────────
    let do_retry = move |_evt: MouseEvent| {
        if submitting() || cooldown_seconds() > 0 {
            return;
        }
        let reason = selected_retry_reason.read().clone();
        if reason.is_empty() {
            banner.set(ResultBanner::Error {
                message: "Pick a retry reason first.".to_string(),
            });
            return;
        }
        // Find the latest non-duplicate original in the recent history
        // for the currently selected section.
        let latest_original = history.read().iter().find(|r| {
            r.retry_sequence == 0 && !r.is_duplicate_attempt
        }).cloned();
        let Some(original) = latest_original else {
            banner.set(ResultBanner::Error {
                message: "No recent original check-in found to retry against."
                    .to_string(),
            });
            return;
        };
        let token = match auth.read().token.clone() {
            Some(t) => t,
            None => return,
        };
        let network_hint = {
            let s = network_hint_input();
            if s.trim().is_empty() { None } else { Some(s) }
        };
        let input = CheckinRetryInput {
            reason_code: reason,
            device_fingerprint: Some(build_device_fingerprint()),
            network_hint,
        };
        submitting.set(true);
        spawn(async move {
            match checkins::retry_checkin(&token, &original.id, &input).await {
                Ok(result) => {
                    submitting.set(false);
                    show_retry_picker.set(false);
                    banner.set(ResultBanner::Success {
                        message: format!(
                            "Retry accepted at {}",
                            result.view.checked_in_at
                        ),
                    });
                    history_reload_tick.with_mut(|n| *n += 1);
                    spawn_cooldown(cooldown_seconds);
                }
                Err(err) => {
                    submitting.set(false);
                    if err.is_unauthorized() {
                        auth.set(AuthState::default());
                        AuthState::clear_storage();
                        navigator.push(AppRoute::Login {});
                        return;
                    }
                    banner.set(ResultBanner::Error { message: err.message });
                }
            }
        });
    };

    // ── Render ──────────────────────────────────────────────────
    let current_banner = banner.read().clone();
    let sections_snapshot = section_list.read().clone();
    let history_snapshot = history.read().clone();
    let retry_snapshot = retry_reasons.read().clone();
    let cooldown = cooldown_seconds();
    let is_submitting = submitting();

    let button_label = if is_submitting {
        "Checking in...".to_string()
    } else if cooldown > 0 {
        format!("Wait {}s", cooldown)
    } else {
        "Check in".to_string()
    };

    rsx! {
        div { class: "page-header",
            h1 { "Check-In" }
            p { class: "text-muted",
                "One tap to record your attendance for the selected section."
            }
        }

        div { class: "checkin-page",
            if can_checkin {
                section { class: "checkin-form",
                    label { r#for: "checkin-section-select", "Section" }
                    if sections_loading() {
                        p { class: "text-muted", "Loading sections..." }
                    } else if let Some(msg) = sections_error() {
                        div { class: "error-banner", "{msg}" }
                    } else if sections_snapshot.is_empty() {
                        p { class: "text-muted", "No sections visible to your account." }
                    } else {
                        select {
                            id: "checkin-section-select",
                            value: "{selected_section_id()}",
                            onchange: move |evt| selected_section_id.set(evt.value()),
                            for s in sections_snapshot.iter().cloned() {
                                option {
                                    key: "{s.id}",
                                    value: "{s.id}",
                                    "{s.course_code} · {s.section_code} ({s.term} {s.year})"
                                }
                            }
                        }
                    }

                    label { r#for: "checkin-network-hint",
                        "Wi-Fi network you're connected to (optional, informational only)"
                    }
                    input {
                        id: "checkin-network-hint",
                        r#type: "text",
                        value: "{network_hint_input()}",
                        placeholder: "e.g. campus-wifi",
                        oninput: move |evt| network_hint_input.set(evt.value()),
                    }

                    button {
                        r#type: "button",
                        class: "checkin-button",
                        disabled: is_submitting || cooldown > 0,
                        onclick: do_checkin,
                        "{button_label}"
                    }
                }

                // ── Result banner ─────────────────────────────
                if let ResultBanner::Success { message } = &current_banner {
                    div { class: "checkin-result-success", "{message}" }
                }
                if let ResultBanner::Duplicate { window_minutes } = &current_banner {
                    div { class: "checkin-result-duplicate",
                        if *window_minutes > 0 {
                            "Duplicate check-in within the {window_minutes} minute window."
                        } else {
                            "Duplicate check-in within the backend-configured window."
                        }
                    }
                }
                if let ResultBanner::NetworkBlocked { message } = &current_banner {
                    div { class: "checkin-result-blocked",
                        p { "Check-in was blocked by the local-network rule." }
                        p { class: "text-muted",
                            "The admin-configured IP/CIDR allowlist is what determines whether your request is considered \"on-campus\"."
                        }
                        p { class: "text-muted", "{message}" }
                    }
                }
                if let ResultBanner::Error { message } = &current_banner {
                    div { class: "error-banner", "{message}" }
                }

                // ── Retry picker ──────────────────────────────
                if show_retry_picker() {
                    div { class: "retry-picker",
                        label { r#for: "checkin-retry-reason", "Retry reason" }
                        select {
                            id: "checkin-retry-reason",
                            value: "{selected_retry_reason()}",
                            onchange: move |evt| selected_retry_reason.set(evt.value()),
                            for reason in retry_snapshot.iter().cloned() {
                                option {
                                    key: "{reason.reason_code}",
                                    value: "{reason.reason_code}",
                                    "{reason.display_name}"
                                }
                            }
                        }
                        button {
                            r#type: "button",
                            class: "primary-button",
                            disabled: is_submitting || cooldown > 0,
                            onclick: do_retry,
                            "Retry"
                        }
                    }
                }
            } else {
                p { class: "text-muted",
                    "You do not have permission to record check-ins. The table below shows recent check-ins for sections you can see."
                }
            }

            // ── History panel ─────────────────────────────────
            section { class: "checkin-history",
                h2 { "Recent check-ins" }
                if let Some(msg) = history_error() {
                    div { class: "error-banner", "{msg}" }
                }
                if history_snapshot.is_empty() {
                    p { class: "text-muted", "No recent check-ins for this section." }
                } else {
                    ul { class: "checkin-history-list",
                        for view in history_snapshot.iter().cloned() {
                            CheckinHistoryRow { key: "{view.id}", view: view.clone() }
                        }
                    }
                }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct CheckinHistoryRowProps {
    view: CheckinView,
}

#[component]
fn CheckinHistoryRow(props: CheckinHistoryRowProps) -> Element {
    let v = props.view.clone();
    let label_extra = if v.is_duplicate_attempt {
        " (duplicate)"
    } else if v.retry_sequence > 0 {
        " (retry)"
    } else {
        ""
    };
    rsx! {
        li { class: "checkin-history-row",
            span { class: "checkin-history-row__user", "{v.user_display}" }
            span { class: "checkin-history-row__type", "{v.checkin_type}{label_extra}" }
            span { class: "text-muted", "{v.checked_in_at}" }
        }
    }
}
