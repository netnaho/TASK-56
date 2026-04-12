//! Bulk import / export workbench.
//!
//! Two tabs — one for courses and one for sections — each supporting
//! template downloads, exports, a dry-run validation pass and a
//! commit-phase upload. The commit button is disabled until the
//! operator has successfully completed a dry-run during the current
//! session.

use dioxus::prelude::*;
use dioxus_router::prelude::Navigator;
use dioxus_router::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::HtmlInputElement;

use crate::api::imports::{
    self, courses_export_csv_url, courses_export_xlsx_url, courses_template_csv_url,
    courses_template_xlsx_url, sections_export_csv_url, sections_export_xlsx_url,
    sections_template_csv_url, sections_template_xlsx_url, ImportReport,
};
use crate::components::dry_run_results::DryRunResults;
use crate::router::AppRoute;
use crate::state::AuthState;
use crate::types::Role;

/// Which tab of the imports workbench is currently active.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ImportsTab {
    Courses,
    Sections,
}

/// Whether the upload is a dry-run or a real commit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    DryRun,
    Commit,
}

impl Mode {
    fn as_str(self) -> &'static str {
        match self {
            Mode::DryRun => "dry_run",
            Mode::Commit => "commit",
        }
    }
}

/// Checks if the authenticated user may access the bulk import page.
fn caller_can_import(auth: &AuthState) -> bool {
    auth.primary_role()
        .map(|r| matches!(r, Role::Admin | Role::DepartmentHead))
        .unwrap_or(false)
}

/// Reads the first file currently selected on an `<input type="file">`
/// element referenced by id.
fn read_selected_file(input_id: &str) -> Option<web_sys::File> {
    let doc = web_sys::window()?.document()?;
    let element = doc.get_element_by_id(input_id)?;
    let input: HtmlInputElement = element.dyn_into().ok()?;
    let files = input.files()?;
    files.get(0)
}

async fn read_file_bytes(file: &web_sys::File) -> Result<Vec<u8>, String> {
    use js_sys::Uint8Array;
    use wasm_bindgen_futures::JsFuture;
    let promise = file.array_buffer();
    let buffer_js = JsFuture::from(promise)
        .await
        .map_err(|e| format!("FileReader failed: {:?}", e))?;
    let buffer: js_sys::ArrayBuffer = buffer_js
        .dyn_into()
        .map_err(|_| "ArrayBuffer cast failed".to_string())?;
    let array = Uint8Array::new(&buffer);
    let mut bytes = vec![0u8; array.length() as usize];
    array.copy_to(&mut bytes);
    Ok(bytes)
}

/// Kicks off an authenticated download. Takes the shared signals and a
/// cloned navigator by value so that multiple button closures can call
/// it without sharing a single outer `FnMut`.
fn start_download(
    mut auth: Signal<AuthState>,
    mut downloading: Signal<bool>,
    mut banner_err: Signal<Option<String>>,
    navigator: Navigator,
    path: String,
    filename: String,
) {
    if downloading() {
        return;
    }
    let token = match auth.read().token.clone() {
        Some(t) => t,
        None => return,
    };
    downloading.set(true);
    banner_err.set(None);
    spawn(async move {
        match imports::download_authenticated(&token, &path, &filename).await {
            Ok(()) => {
                downloading.set(false);
            }
            Err(err) => {
                downloading.set(false);
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
                banner_err.set(Some(format!("Download failed: {}", err.message)));
            }
        }
    });
}

/// Guesses a MIME type for the upload based on the file extension. We
/// let the backend do the real parsing; this is just so the browser
/// sends a reasonable `Content-Type` for the multipart part.
fn guess_mime(file_name: &str) -> &'static str {
    let lower = file_name.to_lowercase();
    if lower.ends_with(".xlsx") {
        "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"
    } else if lower.ends_with(".csv") {
        "text/csv"
    } else {
        "application/octet-stream"
    }
}

#[component]
pub fn Imports() -> Element {
    let mut auth = use_context::<Signal<AuthState>>();
    let navigator = use_navigator();

    let snapshot = auth.read().clone();
    if !caller_can_import(&snapshot) {
        return rsx! {
            div { class: "page-header",
                h1 { "Bulk Import / Export" }
            }
            p { class: "text-muted",
                "You don't have permission to bulk import. This tool is "
                "restricted to department heads and administrators."
            }
        };
    }

    let mut active_tab = use_signal(|| ImportsTab::Courses);
    let mut mode = use_signal(|| Mode::DryRun);
    let mut banner_ok = use_signal(|| Option::<String>::None);
    let mut banner_err = use_signal(|| Option::<String>::None);
    let mut last_report = use_signal(|| Option::<ImportReport>::None);
    // Track, per-tab, whether a successful dry-run has been seen so we
    // can gate the commit button behind a validated preview.
    let mut courses_dry_ok = use_signal(|| false);
    let mut sections_dry_ok = use_signal(|| false);
    let mut busy = use_signal(|| false);
    let mut downloading = use_signal(|| false);

    let current_tab = active_tab();
    let current_mode = mode();

    let tab_label = match current_tab {
        ImportsTab::Courses => "courses",
        ImportsTab::Sections => "sections",
    };
    let input_id = match current_tab {
        ImportsTab::Courses => "imports-courses-file",
        ImportsTab::Sections => "imports-sections-file",
    };

    let can_commit_now = match current_tab {
        ImportsTab::Courses => courses_dry_ok(),
        ImportsTab::Sections => sections_dry_ok(),
    };


    let do_run = move |_| {
        if busy() {
            return;
        }
        let Some(file) = read_selected_file(input_id) else {
            banner_err.set(Some("Please choose a file to upload.".to_string()));
            return;
        };
        let file_name = file.name();
        let mime = guess_mime(&file_name).to_string();
        let token = match auth.read().token.clone() {
            Some(t) => t,
            None => return,
        };
        let tab = current_tab;
        let mode_val = current_mode;
        busy.set(true);
        banner_err.set(None);
        banner_ok.set(None);
        last_report.set(None);
        spawn(async move {
            let bytes = match read_file_bytes(&file).await {
                Ok(b) => b,
                Err(msg) => {
                    busy.set(false);
                    banner_err.set(Some(format!("Failed to read file: {msg}")));
                    return;
                }
            };
            let result = match tab {
                ImportsTab::Courses => {
                    imports::upload_courses_import(
                        &token,
                        &file_name,
                        &mime,
                        bytes,
                        mode_val.as_str(),
                    )
                    .await
                }
                ImportsTab::Sections => {
                    imports::upload_sections_import(
                        &token,
                        &file_name,
                        &mime,
                        bytes,
                        mode_val.as_str(),
                    )
                    .await
                }
            };
            busy.set(false);
            match result {
                Ok(report) => {
                    if report.error_rows > 0 {
                        banner_err.set(Some(format!(
                            "{} row(s) with errors · {} valid · {} total",
                            report.error_rows, report.valid_rows, report.total_rows
                        )));
                        // A failing dry-run invalidates any prior approval.
                        match tab {
                            ImportsTab::Courses => courses_dry_ok.set(false),
                            ImportsTab::Sections => sections_dry_ok.set(false),
                        }
                    } else if report.committed {
                        banner_ok.set(Some(format!(
                            "Committed — {} rows inserted.",
                            report.valid_rows
                        )));
                        // Clear the dry-run gate after a commit so the
                        // next import starts fresh.
                        match tab {
                            ImportsTab::Courses => courses_dry_ok.set(false),
                            ImportsTab::Sections => sections_dry_ok.set(false),
                        }
                    } else {
                        banner_ok.set(Some(format!(
                            "Ready to commit — {} rows validated.",
                            report.valid_rows
                        )));
                        match tab {
                            ImportsTab::Courses => courses_dry_ok.set(true),
                            ImportsTab::Sections => sections_dry_ok.set(true),
                        }
                    }
                    last_report.set(Some(report));
                }
                Err(err) => {
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
                    banner_err.set(Some(format!("Import failed: {}", err.message)));
                }
            }
        });
    };

    let (tpl_csv, tpl_xlsx, exp_csv, exp_xlsx) = match current_tab {
        ImportsTab::Courses => (
            courses_template_csv_url(),
            courses_template_xlsx_url(),
            courses_export_csv_url(),
            courses_export_xlsx_url(),
        ),
        ImportsTab::Sections => (
            sections_template_csv_url(),
            sections_template_xlsx_url(),
            sections_export_csv_url(),
            sections_export_xlsx_url(),
        ),
    };

    // Build four independent click handlers. `Signal`s are `Copy`, so
    // each closure re-captures them freely; `Navigator` only implements
    // `Clone`, so we clone it explicitly into each closure.
    let tpl_csv_click = {
        let path = tpl_csv.clone();
        let filename = format!("{}_template.csv", tab_label);
        let nav = navigator.clone();
        move |_| {
            start_download(auth, downloading, banner_err, nav.clone(), path.clone(), filename.clone());
        }
    };
    let tpl_xlsx_click = {
        let path = tpl_xlsx.clone();
        let filename = format!("{}_template.xlsx", tab_label);
        let nav = navigator.clone();
        move |_| {
            start_download(auth, downloading, banner_err, nav.clone(), path.clone(), filename.clone());
        }
    };
    let exp_csv_click = {
        let path = exp_csv.clone();
        let filename = format!("{}_export.csv", tab_label);
        let nav = navigator.clone();
        move |_| {
            start_download(auth, downloading, banner_err, nav.clone(), path.clone(), filename.clone());
        }
    };
    let exp_xlsx_click = {
        let path = exp_xlsx.clone();
        let filename = format!("{}_export.xlsx", tab_label);
        let nav = navigator.clone();
        move |_| {
            start_download(auth, downloading, banner_err, nav.clone(), path.clone(), filename.clone());
        }
    };

    let courses_tab_class = if current_tab == ImportsTab::Courses {
        "import-tab import-tab--active"
    } else {
        "import-tab"
    };
    let sections_tab_class = if current_tab == ImportsTab::Sections {
        "import-tab import-tab--active"
    } else {
        "import-tab"
    };

    rsx! {
        div { class: "page-header",
            h1 { "Bulk Import / Export" }
            p { class: "text-muted",
                "Download templates, export the catalogue, or import a file with a dry-run first."
            }
        }

        div { class: "import-tabs",
            button {
                r#type: "button",
                class: "{courses_tab_class}",
                onclick: move |_| {
                    active_tab.set(ImportsTab::Courses);
                    last_report.set(None);
                    banner_ok.set(None);
                    banner_err.set(None);
                },
                "Courses"
            }
            button {
                r#type: "button",
                class: "{sections_tab_class}",
                onclick: move |_| {
                    active_tab.set(ImportsTab::Sections);
                    last_report.set(None);
                    banner_ok.set(None);
                    banner_err.set(None);
                },
                "Sections"
            }
        }

        section { class: "import-panel",
            h2 { "Templates & exports" }
            div { class: "export-bar",
                button {
                    r#type: "button",
                    disabled: downloading(),
                    onclick: tpl_csv_click,
                    "Template (CSV)"
                }
                button {
                    r#type: "button",
                    disabled: downloading(),
                    onclick: tpl_xlsx_click,
                    "Template (XLSX)"
                }
                button {
                    r#type: "button",
                    disabled: downloading(),
                    onclick: exp_csv_click,
                    "Export (CSV)"
                }
                button {
                    r#type: "button",
                    disabled: downloading(),
                    onclick: exp_xlsx_click,
                    "Export (XLSX)"
                }
            }
        }

        section { class: "import-panel",
            h2 { "Upload" }
            div { class: "upload-form",
                input {
                    id: "{input_id}",
                    r#type: "file",
                    accept: ".csv,.xlsx",
                    disabled: busy(),
                }
                fieldset { class: "mode-selector",
                    legend { "Mode" }
                    label {
                        input {
                            r#type: "radio",
                            name: "import-mode",
                            checked: current_mode == Mode::DryRun,
                            onchange: move |_| mode.set(Mode::DryRun),
                        }
                        " Dry run"
                    }
                    label {
                        input {
                            r#type: "radio",
                            name: "import-mode",
                            checked: current_mode == Mode::Commit,
                            disabled: !can_commit_now,
                            onchange: move |_| {
                                if can_commit_now {
                                    mode.set(Mode::Commit);
                                }
                            },
                        }
                        " Commit"
                        if !can_commit_now {
                            span { class: "text-muted", " (run a successful dry-run first)" }
                        }
                    }
                }
                button {
                    r#type: "button",
                    class: "primary-button",
                    disabled: busy() || (current_mode == Mode::Commit && !can_commit_now),
                    onclick: do_run,
                    if busy() { "Running..." } else { "Run" }
                }
            }
        }

        if let Some(msg) = banner_err() {
            div { class: "error-banner", "{msg}" }
        }
        if let Some(msg) = banner_ok() {
            if let Some(report) = last_report.read().as_ref() {
                if report.committed {
                    div { class: "info-banner", "{msg}" }
                } else {
                    div { class: "save-banner", "{msg}" }
                }
            } else {
                div { class: "save-banner", "{msg}" }
            }
        }

        if let Some(report) = last_report.read().clone() {
            section { class: "import-panel",
                h2 { "Results" }
                p { class: "text-muted",
                    "Job {report.job_id} · {report.kind} · {report.mode} · "
                    "{report.total_rows} total · {report.valid_rows} valid · "
                    "{report.error_rows} errors"
                }
                DryRunResults { report: report }
            }
        }
    }
}
