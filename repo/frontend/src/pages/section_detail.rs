//! Section detail page — view, edit, approve and publish a single
//! section.

use dioxus::prelude::*;
use dioxus_router::prelude::*;

use crate::api::sections::{self, SectionEditInput, SectionVersionView, SectionView};
use crate::router::AppRoute;
use crate::state::AuthState;
use crate::types::Role;

#[derive(Props, Clone, PartialEq)]
pub struct SectionDetailProps {
    pub id: String,
}

fn caller_can_author(auth: &AuthState) -> bool {
    auth.user
        .as_ref()
        .map(|u| {
            u.roles.iter().any(|r| {
                matches!(
                    r,
                    Role::Admin | Role::DepartmentHead | Role::Instructor
                )
            })
        })
        .unwrap_or(false)
}

#[component]
pub fn SectionDetail(props: SectionDetailProps) -> Element {
    let mut auth = use_context::<Signal<AuthState>>();
    let navigator = use_navigator();

    let section_id = props.id.clone();

    let mut section = use_signal(|| Option::<SectionView>::None);
    let mut versions = use_signal(Vec::<SectionVersionView>::new);
    let mut loading = use_signal(|| true);
    let mut error_msg = use_signal(|| Option::<String>::None);
    let mut save_banner = use_signal(|| Option::<String>::None);

    let mut edit_location = use_signal(String::new);
    let mut edit_schedule_note = use_signal(String::new);
    let mut edit_notes = use_signal(String::new);
    let mut edit_summary = use_signal(String::new);
    let mut edit_error = use_signal(|| Option::<String>::None);
    let mut saving = use_signal(|| false);
    let mut mutating = use_signal(|| false);

    let mut reload_tick = use_signal(|| 0u32);

    let can_author = caller_can_author(&auth.read());

    {
        let id_for_effect = section_id.clone();
        use_effect(move || {
            let token = match auth.read().token.clone() {
                Some(t) => t,
                None => return,
            };
            let _tick = reload_tick.read();
            let id = id_for_effect.clone();
            loading.set(true);
            error_msg.set(None);
            spawn(async move {
                let section_res = sections::get(&token, &id).await;
                let versions_res = sections::list_versions(&token, &id).await;
                loading.set(false);
                match section_res {
                    Ok(s) => {
                        if let Some(eff) = s.effective_version.clone() {
                            edit_location.set(eff.location.clone().unwrap_or_default());
                            edit_schedule_note
                                .set(eff.schedule_note.clone().unwrap_or_default());
                            edit_notes.set(eff.notes.clone().unwrap_or_default());
                            edit_summary.set(String::new());
                        }
                        section.set(Some(s));
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
                        error_msg.set(Some(format!(
                            "Failed to load section: {}",
                            err.message
                        )));
                        return;
                    }
                }
                match versions_res {
                    Ok(vs) => versions.set(vs),
                    Err(err) => {
                        if !err.is_forbidden() {
                            error_msg.set(Some(format!(
                                "Failed to load versions: {}",
                                err.message
                            )));
                        }
                    }
                }
            });
        });
    }

    let section_snapshot = section.read().clone();

    let do_save = {
        let id = section_id.clone();
        move |evt: FormEvent| {
            evt.prevent_default();
            if saving() {
                return;
            }
            let token = match auth.read().token.clone() {
                Some(t) => t,
                None => return,
            };
            let location_val = {
                let s = edit_location();
                if s.trim().is_empty() {
                    None
                } else {
                    Some(s)
                }
            };
            let schedule_val = {
                let s = edit_schedule_note();
                if s.trim().is_empty() {
                    None
                } else {
                    Some(s)
                }
            };
            let notes_val = {
                let s = edit_notes();
                if s.trim().is_empty() {
                    None
                } else {
                    Some(s)
                }
            };
            let summary_val = {
                let s = edit_summary();
                if s.trim().is_empty() {
                    None
                } else {
                    Some(s)
                }
            };
            let input = SectionEditInput {
                location: location_val,
                schedule_note: schedule_val,
                notes: notes_val,
                change_summary: summary_val,
            };
            let id = id.clone();
            saving.set(true);
            edit_error.set(None);
            save_banner.set(None);
            spawn(async move {
                match sections::edit_draft(&token, &id, &input).await {
                    Ok(v) => {
                        saving.set(false);
                        save_banner.set(Some(format!(
                            "Saved as v{} ({})",
                            v.version_number, v.state
                        )));
                        reload_tick.with_mut(|n| *n += 1);
                    }
                    Err(err) => {
                        saving.set(false);
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
                        edit_error.set(Some(err.message));
                    }
                }
            });
        }
    };

    let do_approve = {
        let id = section_id.clone();
        move |_| {
            if mutating() {
                return;
            }
            let Some(s) = section.read().clone() else {
                return;
            };
            let Some(version_id) =
                s.effective_version.as_ref().map(|v| v.id.clone())
            else {
                return;
            };
            let token = match auth.read().token.clone() {
                Some(t) => t,
                None => return,
            };
            let id = id.clone();
            mutating.set(true);
            save_banner.set(None);
            error_msg.set(None);
            spawn(async move {
                match sections::approve(&token, &id, &version_id).await {
                    Ok(v) => {
                        mutating.set(false);
                        save_banner.set(Some(format!(
                            "Approved v{} ({})",
                            v.version_number, v.state
                        )));
                        reload_tick.with_mut(|n| *n += 1);
                    }
                    Err(err) => {
                        mutating.set(false);
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
                        error_msg.set(Some(err.message));
                    }
                }
            });
        }
    };

    let do_publish = {
        let id = section_id.clone();
        move |_| {
            if mutating() {
                return;
            }
            let Some(s) = section.read().clone() else {
                return;
            };
            let Some(version_id) =
                s.effective_version.as_ref().map(|v| v.id.clone())
            else {
                return;
            };
            let token = match auth.read().token.clone() {
                Some(t) => t,
                None => return,
            };
            let id = id.clone();
            mutating.set(true);
            save_banner.set(None);
            error_msg.set(None);
            spawn(async move {
                match sections::publish(&token, &id, &version_id).await {
                    Ok(_) => {
                        mutating.set(false);
                        save_banner.set(Some("Version published.".to_string()));
                        reload_tick.with_mut(|n| *n += 1);
                    }
                    Err(err) => {
                        mutating.set(false);
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
                        error_msg.set(Some(err.message));
                    }
                }
            });
        }
    };

    if loading() {
        return rsx! {
            div { class: "page-header",
                h1 { "Section" }
            }
            p { class: "text-muted", "Loading section..." }
        };
    }

    let Some(s) = section_snapshot.clone() else {
        return rsx! {
            div { class: "page-header",
                h1 { "Section" }
            }
            if let Some(msg) = error_msg() {
                div { class: "error-banner", "{msg}" }
            } else {
                p { class: "text-muted", "Section not found." }
            }
        };
    };

    let effective_state = s
        .effective_version
        .as_ref()
        .map(|v| v.state.clone())
        .unwrap_or_default();
    let effective_version_number = s
        .effective_version
        .as_ref()
        .map(|v| v.version_number)
        .unwrap_or(0);
    let is_published = effective_state == "published";
    let can_approve = can_author && effective_state == "draft";
    let can_publish = can_author && effective_state == "approved";

    let effective_location = s
        .effective_version
        .as_ref()
        .and_then(|v| v.location.clone())
        .unwrap_or_default();
    let effective_schedule = s
        .effective_version
        .as_ref()
        .and_then(|v| v.schedule_note.clone())
        .unwrap_or_default();
    let effective_notes = s
        .effective_version
        .as_ref()
        .and_then(|v| v.notes.clone())
        .unwrap_or_default();
    let capacity_display = s
        .capacity
        .map(|c| c.to_string())
        .unwrap_or_else(|| "-".to_string());
    let instructor_display = s
        .instructor_id
        .clone()
        .unwrap_or_else(|| "-".to_string());

    let course_route = AppRoute::CourseDetail {
        id: s.course_id.clone(),
    };

    rsx! {
        div { class: "page-header library-header",
            div {
                h1 { "{s.course_code} · {s.section_code}" }
                p { class: "text-muted",
                    "{s.term} {s.year} · Created {s.created_at} · Updated {s.updated_at}"
                }
                p {
                    Link { class: "link-button", to: course_route, "← Back to course" }
                }
            }
            div { class: "library-header__actions",
                if is_published {
                    span { class: "version-badge published", "Published" }
                } else {
                    span { class: "version-badge draft", "Draft" }
                }
                if can_approve {
                    button {
                        r#type: "button",
                        class: "primary-button",
                        disabled: mutating(),
                        onclick: do_approve,
                        "Approve v{effective_version_number}"
                    }
                }
                if can_publish {
                    button {
                        r#type: "button",
                        class: "publish-button",
                        disabled: mutating(),
                        onclick: do_publish,
                        "Publish v{effective_version_number}"
                    }
                }
            }
        }

        if let Some(msg) = save_banner() {
            div { class: "save-banner", "{msg}" }
        }
        if let Some(msg) = error_msg() {
            div { class: "error-banner", "{msg}" }
        }

        section { class: "journal-body",
            header { class: "journal-body__header",
                h2 { "Effective version" }
                span { class: "version-badge {effective_state}",
                    "v{effective_version_number} {effective_state}"
                }
            }
            dl { class: "course-meta",
                dt { "Capacity" }
                dd { "{capacity_display}" }
                dt { "Instructor" }
                dd { "{instructor_display}" }
                dt { "Location" }
                dd {
                    if effective_location.is_empty() { "-" } else { "{effective_location}" }
                }
                dt { "Schedule note" }
                dd {
                    if effective_schedule.is_empty() { "-" } else { "{effective_schedule}" }
                }
            }
            h3 { "Notes" }
            if effective_notes.is_empty() {
                p { class: "text-muted", "No notes for this version." }
            } else {
                pre { class: "journal-body__content", "{effective_notes}" }
            }
        }

        if !versions.read().is_empty() {
            section { class: "version-history",
                h2 { "Version history" }
                ul { class: "version-list",
                    for v in versions.read().iter().cloned() {
                        SectionVersionRow { key: "{v.id}", version: v.clone() }
                    }
                }
            }
        }

        if can_author {
            section { class: "edit-panel",
                h2 { "Edit section" }
                p { class: "text-muted",
                    "Saving creates a new draft version. Approved / published versions are immutable."
                }
                if let Some(msg) = edit_error() {
                    div { class: "error-banner", "{msg}" }
                }
                form { class: "edit-form", onsubmit: do_save,
                    label { r#for: "edit-section-location", "Location" }
                    input {
                        id: "edit-section-location",
                        r#type: "text",
                        value: "{edit_location()}",
                        oninput: move |evt| edit_location.set(evt.value()),
                    }
                    label { r#for: "edit-section-schedule", "Schedule note" }
                    input {
                        id: "edit-section-schedule",
                        r#type: "text",
                        value: "{edit_schedule_note()}",
                        oninput: move |evt| edit_schedule_note.set(evt.value()),
                    }
                    label { r#for: "edit-section-notes", "Notes" }
                    textarea {
                        id: "edit-section-notes",
                        rows: "6",
                        value: "{edit_notes()}",
                        oninput: move |evt| edit_notes.set(evt.value()),
                    }
                    label { r#for: "edit-section-summary", "Change summary (optional)" }
                    input {
                        id: "edit-section-summary",
                        r#type: "text",
                        value: "{edit_summary()}",
                        oninput: move |evt| edit_summary.set(evt.value()),
                    }
                    button {
                        r#type: "submit",
                        class: "primary-button",
                        disabled: saving(),
                        if saving() { "Saving..." } else { "Save draft" }
                    }
                }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct SectionVersionRowProps {
    version: SectionVersionView,
}

#[component]
fn SectionVersionRow(props: SectionVersionRowProps) -> Element {
    let v = props.version.clone();
    let state = v.state.clone();
    let created_at = v.created_at.clone();
    let version_number = v.version_number;

    rsx! {
        li { class: "version-row",
            div { class: "version-row__meta",
                span { class: "version-badge {state}", "v{version_number} {state}" }
                span { class: "text-muted", "{created_at}" }
            }
        }
    }
}
