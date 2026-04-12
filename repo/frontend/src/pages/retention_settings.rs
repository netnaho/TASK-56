//! Retention settings page — manage data retention policies.
//!
//! Admins can:
//!   - View all retention policies (entity type, days, action, status).
//!   - Inline-edit `retention_days` (saves on Enter or blur).
//!   - Toggle `is_active` per policy.
//!   - Execute a single policy (dry-run or live).
//!   - Execute all policies at once (dry-run or live).
//!   - View the last execution summary inline.

use dioxus::prelude::*;
use dioxus_router::prelude::*;

use crate::api::retention::{
    self, RetentionExecutionResult, RetentionExecutionSummary, RetentionPolicyView,
    UpdateRetentionPolicyInput,
};
use crate::router::AppRoute;
use crate::state::AuthState;

// ── Helpers ───────────────────────────────────────────────────────────────────

fn action_label(action: &str) -> &'static str {
    match action {
        "delete" => "Delete",
        "anonymize" => "Anonymize",
        "flag_for_review" => "Flag for review",
        _ => "Unknown",
    }
}

fn entity_label(t: &str) -> &'static str {
    match t {
        "audit_logs" => "Audit logs",
        "sessions" => "Sessions",
        "operational_events" => "Operational events",
        "report_runs" => "Report runs",
        _ => "Unknown",
    }
}

// ── Component ─────────────────────────────────────────────────────────────────

#[component]
pub fn RetentionSettings() -> Element {
    let mut auth = use_context::<Signal<AuthState>>();
    let navigator = use_navigator();

    // ── List state ─────────────────────────────────────────────────────
    let mut policies = use_signal(Vec::<RetentionPolicyView>::new);
    let mut loading = use_signal(|| true);
    let mut load_error = use_signal(|| Option::<String>::None);
    let mut reload_tick = use_signal(|| 0u32);

    // ── Per-policy inline edit: maps policy id → draft string ──────────
    // We track which policy is currently being edited and its draft value.
    let mut editing_id = use_signal(|| Option::<String>::None);
    let mut edit_draft = use_signal(String::new);
    let mut save_error = use_signal(|| Option::<String>::None);
    let mut save_banner = use_signal(|| Option::<String>::None);
    let mut saving = use_signal(|| false);

    // ── Execute-all state ─────────────────────────────────────────────
    let mut executing_all = use_signal(|| false);
    let mut exec_all_summary = use_signal(|| Option::<RetentionExecutionSummary>::None);
    let mut exec_all_error = use_signal(|| Option::<String>::None);

    // ── Per-policy execute state ──────────────────────────────────────
    let mut executing_id = use_signal(|| Option::<String>::None);
    let mut per_policy_result = use_signal(|| Option::<RetentionExecutionResult>::None);
    let mut per_policy_error = use_signal(|| Option::<String>::None);

    // ── Load policies ──────────────────────────────────────────────────
    use_effect(move || {
        let _tick = reload_tick.read();
        let token = match auth.read().token.clone() {
            Some(t) => t,
            None => return,
        };
        loading.set(true);
        load_error.set(None);
        spawn(async move {
            match retention::list_policies(&token).await {
                Ok(data) => {
                    policies.set(data);
                    loading.set(false);
                }
                Err(err) => {
                    loading.set(false);
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
                    load_error
                        .set(Some(format!("Failed to load policies: {}", err.message)));
                }
            }
        });
    });

    // ── Snapshots ──────────────────────────────────────────────────────
    let policies_snapshot = policies.read().clone();
    let exec_summary_snap = exec_all_summary.read().clone();
    let per_result_snap = per_policy_result.read().clone();

    rsx! {
        div { class: "page-header library-header",
            div {
                h1 { "Retention Settings" }
                p { class: "text-muted",
                    "Configure how long each category of data is retained before automated action."
                }
            }
        }

        // ── Feedback banners ───────────────────────────────────────────
        if let Some(msg) = save_banner() {
            div { class: "save-banner", "{msg}" }
        }
        if let Some(msg) = save_error() {
            div { class: "error-banner", "{msg}" }
        }
        if let Some(msg) = exec_all_error() {
            div { class: "error-banner", "{msg}" }
        }
        if let Some(msg) = per_policy_error() {
            div { class: "error-banner", "{msg}" }
        }

        // ── Execute-all controls ───────────────────────────────────────
        div { class: "retention-exec-controls",
            button {
                r#type: "button",
                class: "primary-button",
                disabled: executing_all() || saving(),
                onclick: move |_| {
                    if executing_all() { return; }
                    let token = match auth.read().token.clone() {
                        Some(t) => t,
                        None => return,
                    };
                    executing_all.set(true);
                    exec_all_summary.set(None);
                    exec_all_error.set(None);
                    per_policy_result.set(None);
                    spawn(async move {
                        match retention::execute_all(&token, true).await {
                            Ok(summary) => {
                                executing_all.set(false);
                                exec_all_summary.set(Some(summary));
                                reload_tick.with_mut(|n| *n += 1);
                            }
                            Err(err) => {
                                executing_all.set(false);
                                exec_all_error.set(Some(format!("Execution failed: {}", err.message)));
                            }
                        }
                    });
                },
                if executing_all() {
                    "Running dry run…"
                } else {
                    "Execute All (Dry Run)"
                }
            }
            button {
                r#type: "button",
                class: "danger-button",
                disabled: executing_all() || saving(),
                onclick: move |_| {
                    if executing_all() { return; }
                    let token = match auth.read().token.clone() {
                        Some(t) => t,
                        None => return,
                    };
                    executing_all.set(true);
                    exec_all_summary.set(None);
                    exec_all_error.set(None);
                    per_policy_result.set(None);
                    spawn(async move {
                        match retention::execute_all(&token, false).await {
                            Ok(summary) => {
                                executing_all.set(false);
                                exec_all_summary.set(Some(summary));
                                reload_tick.with_mut(|n| *n += 1);
                            }
                            Err(err) => {
                                executing_all.set(false);
                                exec_all_error.set(Some(format!("Execution failed: {}", err.message)));
                            }
                        }
                    });
                },
                if executing_all() {
                    "Executing…"
                } else {
                    "Execute All (Live)"
                }
            }
        }

        // ── All-policies execution summary ─────────────────────────────
        if let Some(summary) = exec_summary_snap {
            div { class: "retention-summary-box",
                h3 {
                    if summary.dry_run {
                        "Dry-Run Execution Summary"
                    } else {
                        "Live Execution Summary"
                    }
                }
                dl { class: "course-meta",
                    dt { "Policies run" }
                    dd { "{summary.policies_run}" }
                    dt { "Policies skipped" }
                    dd { "{summary.policies_skipped}" }
                    dt { "Total rows affected" }
                    dd { "{summary.total_rows_affected}" }
                    if let Some(files) = summary.total_files_deleted {
                        dt { "Files deleted" }
                        dd { "{files}" }
                    }
                    dt { "Executed at" }
                    dd { "{summary.executed_at}" }
                }
                if !summary.results.is_empty() {
                    details { class: "retention-summary-details",
                        summary { "Per-policy results ({summary.results.len()})" }
                        ul { class: "retention-result-list",
                            for r in summary.results.iter() {
                                li { key: "{r.policy_id}",
                                    span { class: "text-muted",
                                        "{entity_label(&r.target_entity_type)}"
                                    }
                                    " — "
                                    span { "{r.rows_affected} rows ({r.action})" }
                                    if let Some(e) = r.error.as_ref() {
                                        span { class: "error-inline", " Error: {e}" }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // ── Per-policy execute result ──────────────────────────────────
        if let Some(result) = per_result_snap {
            div { class: "retention-summary-box",
                h3 {
                    if result.dry_run {
                        "Dry-Run Result"
                    } else {
                        "Live Execution Result"
                    }
                }
                dl { class: "course-meta",
                    dt { "Entity type" }
                    dd { "{entity_label(&result.target_entity_type)}" }
                    dt { "Action" }
                    dd { "{result.action}" }
                    dt { "Rows affected" }
                    dd { "{result.rows_affected}" }
                    if let Some(files) = result.files_deleted {
                        dt { "Files deleted" }
                        dd { "{files}" }
                    }
                    dt { "Executed at" }
                    dd { "{result.executed_at}" }
                    if let Some(e) = result.error.as_ref() {
                        dt { "Error" }
                        dd { class: "error-inline", "{e}" }
                    }
                }
            }
        }

        // ── Policies table ─────────────────────────────────────────────
        if loading() {
            p { class: "text-muted", "Loading policies…" }
        } else if let Some(msg) = load_error() {
            div { class: "error-banner", "{msg}" }
        } else if policies_snapshot.is_empty() {
            p { class: "text-muted", "No retention policies configured." }
        } else {
            div { class: "reports-table-wrapper",
                table { class: "data-table",
                    thead {
                        tr {
                            th { "Entity type" }
                            th { "Retention (days)" }
                            th { "Action" }
                            th { "Active" }
                            th { "Eligible rows" }
                            th { "Last executed" }
                            th { "" }
                        }
                    }
                    tbody {
                        for policy in policies_snapshot.iter().cloned() {
                            PolicyRow {
                                key: "{policy.id}",
                                policy: policy.clone(),
                                editing_id: editing_id,
                                executing_id: executing_id,
                                executing_all: executing_all,
                                saving: saving,
                                edit_draft: edit_draft,
                                save_error: save_error,
                                per_policy_result: per_policy_result,
                                per_policy_error: per_policy_error,
                                exec_all_summary: exec_all_summary,
                                auth: auth,
                                reload_tick: reload_tick,
                                save_banner: save_banner,
                            }
                        }
                    }
                }
            }

            // ── Action legend ──────────────────────────────────────────
            div { class: "retention-legend",
                p {
                    span { class: "tag tag--danger", "Delete" }
                    " — permanently removes rows from the database."
                }
                p {
                    span { class: "tag tag--warning", "Anonymize" }
                    " ⚠ — replaces personally identifiable fields with pseudonymous tokens. Rows are kept but cannot be traced back to individuals."
                }
                p {
                    span { class: "tag", "Flag for review" }
                    " — marks rows as eligible for manual review without modifying data."
                }
            }
        }
    }
}

// ── PolicyRow sub-component ───────────────────────────────────────────────────

#[derive(Props, Clone, PartialEq)]
struct PolicyRowProps {
    policy: RetentionPolicyView,
    editing_id: Signal<Option<String>>,
    executing_id: Signal<Option<String>>,
    executing_all: Signal<bool>,
    saving: Signal<bool>,
    edit_draft: Signal<String>,
    save_error: Signal<Option<String>>,
    per_policy_result: Signal<Option<RetentionExecutionResult>>,
    per_policy_error: Signal<Option<String>>,
    exec_all_summary: Signal<Option<RetentionExecutionSummary>>,
    auth: Signal<AuthState>,
    reload_tick: Signal<u32>,
    save_banner: Signal<Option<String>>,
}

#[component]
fn PolicyRow(props: PolicyRowProps) -> Element {
    let policy = props.policy.clone();
    let mut editing_id = props.editing_id;
    let mut executing_id = props.executing_id;
    let executing_all = props.executing_all;
    let mut saving = props.saving;
    let mut edit_draft = props.edit_draft;
    let mut save_error = props.save_error;
    let mut per_policy_result = props.per_policy_result;
    let mut per_policy_error = props.per_policy_error;
    let mut exec_all_summary = props.exec_all_summary;
    let mut auth = props.auth;
    let mut reload_tick = props.reload_tick;
    let mut save_banner = props.save_banner;

    let policy_id = policy.id.clone();
    let entity = entity_label(&policy.target_entity_type).to_string();
    let action = policy.action.clone();
    let action_display = action_label(&action).to_string();
    let is_active = policy.is_active;
    let days = policy.retention_days;
    let eligible = policy
        .eligible_rows
        .map(|n| n.to_string())
        .unwrap_or_else(|| "—".to_string());
    let last_exec = policy
        .last_executed_at
        .clone()
        .unwrap_or_else(|| "Never".to_string());
    let is_editing = editing_id.read().as_deref() == Some(&policy_id);
    let is_executing = executing_id.read().as_deref() == Some(&policy_id);
    let is_anonymize = action == "anonymize";

    let save_id = policy_id.clone();
    let toggle_id = policy_id.clone();
    let exec_id = policy_id.clone();

    // ── Pre-computed class strings (avoids E0283 type inference in rsx!) ──
    let action_class: &str = if action == "delete" {
        "tag tag--danger"
    } else if is_anonymize {
        "tag tag--warning"
    } else {
        "tag"
    };
    let toggle_class: &str = if is_active {
        "toggle-btn toggle-btn--on"
    } else {
        "toggle-btn toggle-btn--off"
    };

    // ── Toggle is_active helper ────────────────────────────────────────
    let mut do_toggle_active = move |policy_id: String, new_state: bool| {
        if saving() {
            return;
        }
        let token = match auth.read().token.clone() {
            Some(t) => t,
            None => return,
        };
        saving.set(true);
        save_banner.set(None);
        save_error.set(None);
        spawn(async move {
            let input = UpdateRetentionPolicyInput {
                is_active: Some(new_state),
                ..Default::default()
            };
            match retention::update_policy(&token, &policy_id, &input).await {
                Ok(_) => {
                    saving.set(false);
                    save_banner.set(Some(if new_state {
                        "Policy activated.".to_string()
                    } else {
                        "Policy deactivated.".to_string()
                    }));
                    reload_tick.with_mut(|n| *n += 1);
                }
                Err(err) => {
                    saving.set(false);
                    save_error.set(Some(format!("Toggle failed: {}", err.message)));
                }
            }
        });
    };

    rsx! {
        tr { key: "{policy_id}",
            // Entity type
            td { class: "data-table__cell--primary", "{entity}" }

            // Retention days — inline editable
            td {
                if is_editing {
                    input {
                        r#type: "number",
                        class: "inline-input",
                        min: "1",
                        value: "{edit_draft()}",
                        oninput: move |evt| edit_draft.set(evt.value()),
                        onkeydown: {
                            let sid = save_id.clone();
                            move |evt: KeyboardEvent| {
                                if evt.key() == Key::Enter {
                                    if saving() { return; }
                                    let draft = edit_draft().trim().to_string();
                                    let days_val: i32 = match draft.parse() {
                                        Ok(n) if n > 0 => n,
                                        _ => {
                                            save_error.set(Some("Retention days must be a positive integer.".to_string()));
                                            return;
                                        }
                                    };
                                    let token = match auth.read().token.clone() {
                                        Some(t) => t,
                                        None => return,
                                    };
                                    saving.set(true);
                                    save_error.set(None);
                                    let pid = sid.clone();
                                    spawn(async move {
                                        let input = UpdateRetentionPolicyInput {
                                            retention_days: Some(days_val),
                                            ..Default::default()
                                        };
                                        match retention::update_policy(&token, &pid, &input).await {
                                            Ok(_) => {
                                                saving.set(false);
                                                editing_id.set(None);
                                                save_banner.set(Some("Retention days updated.".to_string()));
                                                reload_tick.with_mut(|n| *n += 1);
                                            }
                                            Err(err) => {
                                                saving.set(false);
                                                save_error.set(Some(format!("Save failed: {}", err.message)));
                                            }
                                        }
                                    });
                                }
                                if evt.key() == Key::Escape {
                                    editing_id.set(None);
                                }
                            }
                        },
                        onblur: {
                            let sid = save_id.clone();
                            move |_: Event<FocusData>| {
                                if saving() { return; }
                                let draft = edit_draft().trim().to_string();
                                let days_val: i32 = match draft.parse() {
                                    Ok(n) if n > 0 => n,
                                    _ => {
                                        save_error.set(Some("Retention days must be a positive integer.".to_string()));
                                        return;
                                    }
                                };
                                let token = match auth.read().token.clone() {
                                    Some(t) => t,
                                    None => return,
                                };
                                saving.set(true);
                                save_error.set(None);
                                let pid = sid.clone();
                                spawn(async move {
                                    let input = UpdateRetentionPolicyInput {
                                        retention_days: Some(days_val),
                                        ..Default::default()
                                    };
                                    match retention::update_policy(&token, &pid, &input).await {
                                        Ok(_) => {
                                            saving.set(false);
                                            editing_id.set(None);
                                            save_banner.set(Some("Retention days updated.".to_string()));
                                            reload_tick.with_mut(|n| *n += 1);
                                        }
                                        Err(err) => {
                                            saving.set(false);
                                            save_error.set(Some(format!("Save failed: {}", err.message)));
                                        }
                                    }
                                });
                            }
                        },
                    }
                } else {
                    button {
                        r#type: "button",
                        class: "link-button retention-days-btn",
                        title: "Click to edit",
                        onclick: {
                            let pid = policy_id.clone();
                            move |_: Event<MouseData>| {
                                editing_id.set(Some(pid.clone()));
                                edit_draft.set(days.to_string());
                                save_error.set(None);
                            }
                        },
                        "{days}"
                    }
                }
            }

            // Action — with warning badge for destructive actions
            td {
                span {
                    class: action_class,
                    "{action_display}"
                }
                if is_anonymize {
                    span {
                        class: "retention-warn",
                        title: "Anonymize replaces PII with pseudonymous values but keeps the row.",
                        " ⚠"
                    }
                }
            }

            // Active toggle
            td {
                button {
                    r#type: "button",
                    class: toggle_class,
                    disabled: saving(),
                    onclick: {
                        let tid = toggle_id.clone();
                        move |_: Event<MouseData>| { do_toggle_active(tid.clone(), !is_active); }
                    },
                    if is_active { "Active" } else { "Inactive" }
                }
            }

            // Eligible rows
            td { class: "text-muted", "{eligible}" }

            // Last executed
            td { class: "text-muted", "{last_exec}" }

            // Per-policy execute
            td { class: "retention-exec-cell",
                button {
                    r#type: "button",
                    class: "link-button",
                    disabled: is_executing || executing_all(),
                    title: "Dry run — preview only",
                    onclick: {
                        let eid = exec_id.clone();
                        move |_: Event<MouseData>| {
                            if executing_id.read().is_some() || executing_all() {
                                return;
                            }
                            let token = match auth.read().token.clone() {
                                Some(t) => t,
                                None => return,
                            };
                            executing_id.set(Some(eid.clone()));
                            per_policy_result.set(None);
                            per_policy_error.set(None);
                            exec_all_summary.set(None);
                            let eid_async = eid.clone();
                            spawn(async move {
                                match retention::execute_policy(&token, &eid_async, true).await {
                                    Ok(result) => {
                                        executing_id.set(None);
                                        per_policy_result.set(Some(result));
                                    }
                                    Err(err) => {
                                        executing_id.set(None);
                                        per_policy_error.set(Some(format!(
                                            "Execute failed: {}",
                                            err.message
                                        )));
                                    }
                                }
                            });
                        }
                    },
                    if is_executing { "Running…" } else { "Dry run" }
                }
                span { class: "text-muted", " · " }
                button {
                    r#type: "button",
                    class: "danger-button danger-button--sm",
                    disabled: is_executing || executing_all(),
                    title: "Execute live — commits changes",
                    onclick: {
                        let eid = exec_id.clone();
                        move |_: Event<MouseData>| {
                            if executing_id.read().is_some() || executing_all() {
                                return;
                            }
                            let token = match auth.read().token.clone() {
                                Some(t) => t,
                                None => return,
                            };
                            executing_id.set(Some(eid.clone()));
                            per_policy_result.set(None);
                            per_policy_error.set(None);
                            exec_all_summary.set(None);
                            let eid_async = eid.clone();
                            spawn(async move {
                                match retention::execute_policy(&token, &eid_async, false).await {
                                    Ok(result) => {
                                        executing_id.set(None);
                                        per_policy_result.set(Some(result));
                                        reload_tick.with_mut(|n| *n += 1);
                                    }
                                    Err(err) => {
                                        executing_id.set(None);
                                        per_policy_error.set(Some(format!(
                                            "Execute failed: {}",
                                            err.message
                                        )));
                                    }
                                }
                            });
                        }
                    },
                    if is_executing { "Running…" } else { "Execute" }
                }
            }
        }
    }
}
