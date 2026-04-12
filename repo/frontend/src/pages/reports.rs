//! Reports page — manage report definitions, view run history, and
//! administer schedules.
//!
//! Three sections:
//!   1. Report Definitions — list with per-report "Run" button.
//!   2. Recent Runs       — table with status, size, source, download.
//!   3. Active Schedules  — list with "Disable" per schedule.

use dioxus::prelude::*;
use dioxus_router::prelude::*;
use js_sys::Uint8Array;
use wasm_bindgen::JsCast;
use web_sys::{Blob, BlobPropertyBag, HtmlAnchorElement, Url};

use crate::api::reports::{
    self, artifact_download_url, ReportRunView, ReportScheduleView, ReportView,
};
use crate::router::AppRoute;
use crate::state::AuthState;

// ── Download helper ──────────────────────────────────────────────────────────

/// Fetches a run artifact via `gloo-net` (so the `Authorization` header
/// is attached), then triggers a browser file-save using a blob URL.
async fn download_artifact(token: String, run_id: String, filename: String) {
    let url = artifact_download_url(&run_id);
    let Ok(response) = gloo_net::http::Request::get(&url)
        .header("Authorization", &format!("Bearer {}", token))
        .send()
        .await
    else {
        return;
    };

    let Ok(bytes) = response.binary().await else {
        return;
    };

    let uint8 = Uint8Array::from(bytes.as_slice());
    let array = js_sys::Array::new();
    array.push(&uint8.buffer());

    let blob_opts = BlobPropertyBag::new();
    let Ok(blob) = Blob::new_with_u8_array_sequence_and_options(&array, &blob_opts) else {
        return;
    };

    let Ok(obj_url) = Url::create_object_url_with_blob(&blob) else {
        return;
    };

    if let Some(document) = web_sys::window().and_then(|w| w.document()) {
        if let Ok(el) = document.create_element("a") {
            let a: HtmlAnchorElement = el.unchecked_into();
            a.set_href(&obj_url);
            a.set_download(&filename);
            a.click();
        }
    }

    let _ = Url::revoke_object_url(&obj_url);
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn short_id(id: &str) -> &str {
    &id[..id.len().min(8)]
}

fn format_bytes(bytes: i64) -> String {
    if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}

fn status_badge_class(status: &str) -> &'static str {
    match status {
        "completed" => "status-badge status-badge--completed",
        "failed" => "status-badge status-badge--failed",
        "running" => "status-badge status-badge--running",
        "queued" => "status-badge status-badge--queued",
        "cancelled" => "status-badge status-badge--cancelled",
        _ => "status-badge",
    }
}

// ── Page component ────────────────────────────────────────────────────────────

#[component]
pub fn Reports() -> Element {
    let mut auth = use_context::<Signal<AuthState>>();
    let navigator = use_navigator();

    // ── Reports list state ─────────────────────────────────────────────
    let mut reports_list = use_signal(Vec::<ReportView>::new);
    let mut reports_loading = use_signal(|| true);
    let mut reports_error = use_signal(|| Option::<String>::None);

    // ── Runs list state ────────────────────────────────────────────────
    let mut runs_list = use_signal(Vec::<ReportRunView>::new);
    let mut runs_loading = use_signal(|| true);
    let mut runs_error = use_signal(|| Option::<String>::None);
    let mut runs_reload = use_signal(|| 0u32);

    // ── Schedules list state ───────────────────────────────────────────
    let mut schedules_list = use_signal(Vec::<ReportScheduleView>::new);
    let mut schedules_loading = use_signal(|| true);
    let mut schedules_error = use_signal(|| Option::<String>::None);
    let mut schedules_reload = use_signal(|| 0u32);

    // ── Per-report action state ────────────────────────────────────────
    let mut triggering = use_signal(|| Option::<String>::None); // report id being triggered
    let mut trigger_banner = use_signal(|| Option::<String>::None);

    // ── Per-schedule action state ──────────────────────────────────────
    let mut disabling = use_signal(|| Option::<String>::None); // schedule id being disabled
    let mut disable_banner = use_signal(|| Option::<String>::None);

    // ── Downloading state ──────────────────────────────────────────────
    let mut downloading = use_signal(|| Option::<String>::None); // run id being downloaded

    // ── Load reports ───────────────────────────────────────────────────
    use_effect(move || {
        let token = match auth.read().token.clone() {
            Some(t) => t,
            None => return,
        };
        reports_loading.set(true);
        reports_error.set(None);
        spawn(async move {
            match reports::list_reports(&token).await {
                Ok(data) => {
                    reports_list.set(data);
                    reports_loading.set(false);
                }
                Err(err) => {
                    reports_loading.set(false);
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
                    reports_error.set(Some(format!("Failed to load reports: {}", err.message)));
                }
            }
        });
    });

    // ── Load recent runs ───────────────────────────────────────────────
    use_effect(move || {
        let _tick = runs_reload.read();
        let token = match auth.read().token.clone() {
            Some(t) => t,
            None => return,
        };
        runs_loading.set(true);
        runs_error.set(None);
        spawn(async move {
            match reports::list_all_runs(&token).await {
                Ok(data) => {
                    runs_list.set(data);
                    runs_loading.set(false);
                }
                Err(err) => {
                    runs_loading.set(false);
                    if err.is_unauthorized() {
                        auth.set(AuthState::default());
                        AuthState::clear_storage();
                        navigator.push(AppRoute::Login {});
                        return;
                    }
                    runs_error.set(Some(format!("Failed to load runs: {}", err.message)));
                }
            }
        });
    });

    // ── Load schedules ─────────────────────────────────────────────────
    use_effect(move || {
        let _tick = schedules_reload.read();
        let token = match auth.read().token.clone() {
            Some(t) => t,
            None => return,
        };
        schedules_loading.set(true);
        schedules_error.set(None);
        spawn(async move {
            match reports::list_all_schedules(&token).await {
                Ok(data) => {
                    schedules_list.set(data);
                    schedules_loading.set(false);
                }
                Err(err) => {
                    schedules_loading.set(false);
                    if err.is_unauthorized() {
                        auth.set(AuthState::default());
                        AuthState::clear_storage();
                        navigator.push(AppRoute::Login {});
                        return;
                    }
                    schedules_error
                        .set(Some(format!("Failed to load schedules: {}", err.message)));
                }
            }
        });
    });

    // ── Snapshots for rendering ────────────────────────────────────────
    let reports_snapshot = reports_list.read().clone();
    let runs_snapshot = runs_list.read().clone();
    let schedules_snapshot = schedules_list.read().clone();

    rsx! {
        div { class: "page-header library-header",
            div {
                h1 { "Reports" }
                p { class: "text-muted",
                    "Define, run, and schedule data exports for analysis."
                }
            }
        }

        // ── Trigger / disable banners ──────────────────────────────────
        if let Some(msg) = trigger_banner() {
            div { class: "save-banner", "{msg}" }
        }
        if let Some(msg) = disable_banner() {
            div { class: "save-banner", "{msg}" }
        }

        // ══════════════════════════════════════════════════════════════
        // Section 1 — Report Definitions
        // ══════════════════════════════════════════════════════════════
        section { class: "reports-section",
            h2 { class: "reports-section__title", "Report Definitions" }

            if reports_loading() {
                p { class: "text-muted", "Loading reports…" }
            } else if let Some(msg) = reports_error() {
                div { class: "error-banner", "{msg}" }
            } else if reports_snapshot.is_empty() {
                p { class: "text-muted", "No report definitions found." }
            } else {
                div { class: "reports-table-wrapper",
                    table { class: "data-table",
                        thead {
                            tr {
                                th { "Title" }
                                th { "Type" }
                                th { "Format" }
                                th { "Description" }
                                th { "" }
                            }
                        }
                        tbody {
                            for report in reports_snapshot.iter().cloned() {
                                {
                                    let report_id = report.id.clone();
                                    let trigger_id = report_id.clone();
                                    let title = report.title.clone();
                                    let rtype = report.query_definition.report_type.clone();
                                    let fmt = report.default_format.clone();
                                    let desc = report.description.clone().unwrap_or_default();
                                    let is_triggering = triggering.read().as_deref() == Some(&report_id);

                                    rsx! {
                                        tr { key: "{report_id}",
                                            td { class: "data-table__cell--primary", "{title}" }
                                            td {
                                                span { class: "tag", "{rtype}" }
                                            }
                                            td {
                                                span { class: "tag tag--format", "{fmt}" }
                                            }
                                            td { class: "text-muted",
                                                if desc.is_empty() {
                                                    span { class: "text-muted", "—" }
                                                } else {
                                                    "{desc}"
                                                }
                                            }
                                            td {
                                                button {
                                                    r#type: "button",
                                                    class: "primary-button primary-button--sm",
                                                    disabled: is_triggering,
                                                    onclick: move |_| {
                                                        if triggering.read().is_some() {
                                                            return;
                                                        }
                                                        let t_id = trigger_id.clone();
                                                        let token = match auth.read().token.clone() {
                                                            Some(tk) => tk,
                                                            None => return,
                                                        };
                                                        triggering.set(Some(t_id.clone()));
                                                        trigger_banner.set(None);
                                                        spawn(async move {
                                                            match reports::trigger_run(&token, &t_id, None).await {
                                                                Ok(run) => {
                                                                    triggering.set(None);
                                                                    trigger_banner.set(Some(format!(
                                                                        "Run queued (id: {}).",
                                                                        short_id(&run.id)
                                                                    )));
                                                                    runs_reload.with_mut(|n| *n += 1);
                                                                }
                                                                Err(err) => {
                                                                    triggering.set(None);
                                                                    trigger_banner.set(Some(format!(
                                                                        "Failed to trigger run: {}",
                                                                        err.message
                                                                    )));
                                                                }
                                                            }
                                                        });
                                                    },
                                                    if is_triggering { "Running…" } else { "Run" }
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

        // ══════════════════════════════════════════════════════════════
        // Section 2 — Recent Runs
        // ══════════════════════════════════════════════════════════════
        section { class: "reports-section",
            div { class: "library-header",
                h2 { class: "reports-section__title", "Recent Runs" }
                button {
                    r#type: "button",
                    class: "link-button",
                    onclick: move |_| runs_reload.with_mut(|n| *n += 1),
                    "Refresh"
                }
            }

            if runs_loading() {
                p { class: "text-muted", "Loading runs…" }
            } else if let Some(msg) = runs_error() {
                div { class: "error-banner", "{msg}" }
            } else if runs_snapshot.is_empty() {
                p { class: "text-muted", "No runs recorded yet." }
            } else {
                div { class: "reports-table-wrapper",
                    table { class: "data-table",
                        thead {
                            tr {
                                th { "Run ID" }
                                th { "Report" }
                                th { "Status" }
                                th { "Format" }
                                th { "Size" }
                                th { "Source" }
                                th { "Completed" }
                                th { "" }
                            }
                        }
                        tbody {
                            for run in runs_snapshot.iter().cloned() {
                                {
                                    let run_id = run.id.clone();
                                    let dl_run_id = run_id.clone();
                                    let short = short_id(&run_id).to_string();
                                    let title = run.report_title.clone();
                                    let status = run.status.clone();
                                    let status_class = status_badge_class(&status).to_string();
                                    let fmt = run.format.clone();
                                    let size = run
                                        .artifact_size_bytes
                                        .map(format_bytes)
                                        .unwrap_or_else(|| "—".to_string());
                                    let source = run.triggered_source.clone();
                                    let completed = run
                                        .completed_at
                                        .clone()
                                        .unwrap_or_else(|| "—".to_string());
                                    let can_download = run.artifact_available;
                                    let is_downloading =
                                        downloading.read().as_deref() == Some(&run_id);
                                    let dl_filename =
                                        format!("report-{}.{}", short_id(&run_id), fmt.clone());

                                    rsx! {
                                        tr { key: "{run_id}",
                                            td {
                                                span { class: "monospace text-muted", "{short}…" }
                                            }
                                            td { class: "data-table__cell--primary", "{title}" }
                                            td {
                                                span { class: "{status_class}", "{status}" }
                                            }
                                            td {
                                                span { class: "tag tag--format", "{fmt}" }
                                            }
                                            td { class: "text-muted", "{size}" }
                                            td { class: "text-muted", "{source}" }
                                            td { class: "text-muted", "{completed}" }
                                            td {
                                                if can_download {
                                                    button {
                                                        r#type: "button",
                                                        class: "link-button",
                                                        disabled: is_downloading,
                                                        onclick: move |_| {
                                                            let token = match auth.read().token.clone() {
                                                                Some(tk) => tk,
                                                                None => return,
                                                            };
                                                            let rid = dl_run_id.clone();
                                                            let fname = dl_filename.clone();
                                                            downloading.set(Some(rid.clone()));
                                                            spawn(async move {
                                                                download_artifact(token, rid.clone(), fname).await;
                                                                downloading.with_mut(|d| {
                                                                    if d.as_deref() == Some(&rid) {
                                                                        *d = None;
                                                                    }
                                                                });
                                                            });
                                                        },
                                                        if is_downloading { "Downloading…" } else { "Download" }
                                                    }
                                                } else {
                                                    span { class: "text-muted", "—" }
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

        // ══════════════════════════════════════════════════════════════
        // Section 3 — Active Schedules
        // ══════════════════════════════════════════════════════════════
        section { class: "reports-section",
            div { class: "library-header",
                h2 { class: "reports-section__title", "Active Schedules" }
                button {
                    r#type: "button",
                    class: "link-button",
                    onclick: move |_| schedules_reload.with_mut(|n| *n += 1),
                    "Refresh"
                }
            }

            if schedules_loading() {
                p { class: "text-muted", "Loading schedules…" }
            } else if let Some(msg) = schedules_error() {
                div { class: "error-banner", "{msg}" }
            } else if schedules_snapshot.is_empty() {
                p { class: "text-muted", "No active schedules configured." }
            } else {
                div { class: "reports-table-wrapper",
                    table { class: "data-table",
                        thead {
                            tr {
                                th { "Report" }
                                th { "Cron" }
                                th { "Format" }
                                th { "Next Run" }
                                th { "Last Run" }
                                th { "" }
                            }
                        }
                        tbody {
                            for sched in schedules_snapshot.iter().cloned() {
                                {
                                    let sched_id = sched.id.clone();
                                    let disable_id = sched_id.clone();
                                    let title = sched.report_title.clone();
                                    let cron = sched.cron_expression.clone();
                                    let fmt = sched.format.clone();
                                    let next = sched
                                        .next_run_at
                                        .clone()
                                        .unwrap_or_else(|| "—".to_string());
                                    let last = sched
                                        .last_run_at
                                        .clone()
                                        .unwrap_or_else(|| "Never".to_string());
                                    let is_disabling =
                                        disabling.read().as_deref() == Some(&sched_id);

                                    rsx! {
                                        tr { key: "{sched_id}",
                                            td { class: "data-table__cell--primary", "{title}" }
                                            td {
                                                code { class: "monospace", "{cron}" }
                                            }
                                            td {
                                                span { class: "tag tag--format", "{fmt}" }
                                            }
                                            td { class: "text-muted", "{next}" }
                                            td { class: "text-muted", "{last}" }
                                            td {
                                                button {
                                                    r#type: "button",
                                                    class: "danger-button danger-button--sm",
                                                    disabled: is_disabling,
                                                    onclick: move |_| {
                                                        if disabling.read().is_some() {
                                                            return;
                                                        }
                                                        let s_id = disable_id.clone();
                                                        let token = match auth.read().token.clone() {
                                                            Some(tk) => tk,
                                                            None => return,
                                                        };
                                                        disabling.set(Some(s_id.clone()));
                                                        disable_banner.set(None);
                                                        spawn(async move {
                                                            match reports::delete_schedule(&token, &s_id).await {
                                                                Ok(()) => {
                                                                    disabling.set(None);
                                                                    disable_banner.set(Some(
                                                                        "Schedule disabled.".to_string(),
                                                                    ));
                                                                    schedules_reload
                                                                        .with_mut(|n| *n += 1);
                                                                }
                                                                Err(err) => {
                                                                    disabling.set(None);
                                                                    disable_banner.set(Some(format!(
                                                                        "Failed to disable schedule: {}",
                                                                        err.message
                                                                    )));
                                                                }
                                                            }
                                                        });
                                                    },
                                                    if is_disabling { "Disabling…" } else { "Disable" }
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
