//! Journal detail page — view, edit, approve, and publish a single
//! journal, plus manage its attachments.

use dioxus::prelude::*;
use dioxus_router::prelude::*;

use crate::api::attachments::ParentType;
use crate::api::journals::{self, JournalEditInput, JournalVersionView, JournalView};
use crate::components::attachment_panel::{AttachmentPanel, AttachmentPanelProps};
use crate::router::AppRoute;
use crate::state::AuthState;
use crate::types::Role;

#[derive(Props, Clone, PartialEq)]
pub struct JournalDetailProps {
    pub id: String,
}

fn caller_can_author(auth: &AuthState) -> bool {
    auth.user
        .as_ref()
        .map(|u| {
            u.roles
                .iter()
                .any(|r| matches!(r, Role::Admin | Role::Librarian))
        })
        .unwrap_or(false)
}

#[component]
pub fn JournalDetail(props: JournalDetailProps) -> Element {
    let mut auth = use_context::<Signal<AuthState>>();
    let navigator = use_navigator();

    let journal_id = props.id.clone();

    let mut journal = use_signal(|| Option::<JournalView>::None);
    let mut versions = use_signal(Vec::<JournalVersionView>::new);
    let mut selected_version = use_signal(|| Option::<JournalVersionView>::None);
    let mut loading = use_signal(|| true);
    let mut error_msg = use_signal(|| Option::<String>::None);
    let mut save_banner = use_signal(|| Option::<String>::None);

    let mut edit_title = use_signal(String::new);
    let mut edit_body = use_signal(String::new);
    let mut edit_summary = use_signal(String::new);
    let mut edit_error = use_signal(|| Option::<String>::None);
    let mut saving = use_signal(|| false);
    let mut mutating = use_signal(|| false);

    let mut reload_tick = use_signal(|| 0u32);

    let can_author = caller_can_author(&auth.read());

    {
        let id_for_effect = journal_id.clone();
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
                let journal_res = journals::get(&token, &id).await;
                let versions_res = journals::list_versions(&token, &id).await;
                loading.set(false);
                match journal_res {
                    Ok(j) => {
                        if let Some(eff) = j.effective_version.clone() {
                            edit_title.set(j.title.clone());
                            edit_body.set(eff.body.clone().unwrap_or_default());
                            edit_summary.set(String::new());
                        } else {
                            edit_title.set(j.title.clone());
                            edit_body.set(String::new());
                            edit_summary.set(String::new());
                        }
                        journal.set(Some(j));
                    }
                    Err(err) => {
                        if err.is_unauthorized() {
                            auth.set(AuthState::default());
                            AuthState::clear_storage();
                            navigator.push(AppRoute::Login {});
                            return;
                        }
                        error_msg.set(Some(format!("Failed to load journal: {}", err.message)));
                        return;
                    }
                }
                match versions_res {
                    Ok(vs) => {
                        versions.set(vs);
                        selected_version.set(None);
                    }
                    Err(err) => {
                        if !err.is_forbidden() {
                            error_msg
                                .set(Some(format!("Failed to load versions: {}", err.message)));
                        }
                    }
                }
            });
        });
    }

    let journal_snapshot = journal.read().clone();

    // Effective body source — if the user picked a version manually,
    // show that; otherwise fall back to the journal's effective_version.
    let (displayed_title, displayed_body, displayed_state, displayed_version_number) = {
        let selected = selected_version.read().clone();
        if let Some(v) = selected {
            (
                v.title.unwrap_or_default(),
                v.body.unwrap_or_default(),
                v.state,
                Some(v.version_number),
            )
        } else if let Some(j) = journal_snapshot.as_ref() {
            let eff = j.effective_version.clone();
            (
                j.title.clone(),
                eff.as_ref()
                    .and_then(|v| v.body.clone())
                    .unwrap_or_default(),
                eff.as_ref()
                    .map(|v| v.state.clone())
                    .unwrap_or_else(|| "-".to_string()),
                eff.as_ref().map(|v| v.version_number),
            )
        } else {
            (String::new(), String::new(), "-".to_string(), None)
        }
    };

    let do_save = {
        let id = journal_id.clone();
        move |evt: FormEvent| {
            evt.prevent_default();
            if saving() {
                return;
            }
            let title = edit_title().trim().to_string();
            let body = edit_body();
            if !title.is_empty() && title.len() < 3 {
                edit_error.set(Some("Title must be at least 3 characters.".to_string()));
                return;
            }
            if body.trim().is_empty() {
                edit_error.set(Some("Body must not be empty.".to_string()));
                return;
            }
            let token = match auth.read().token.clone() {
                Some(t) => t,
                None => return,
            };
            let summary_val = {
                let s = edit_summary();
                if s.trim().is_empty() {
                    None
                } else {
                    Some(s)
                }
            };
            let input = JournalEditInput {
                title: if title.is_empty() { None } else { Some(title) },
                body,
                change_summary: summary_val,
            };
            let id = id.clone();
            saving.set(true);
            edit_error.set(None);
            save_banner.set(None);
            spawn(async move {
                match journals::edit(&token, &id, &input).await {
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
                        edit_error.set(Some(err.message));
                    }
                }
            });
        }
    };

    let do_approve = {
        let id = journal_id.clone();
        move |_| {
            if mutating() {
                return;
            }
            let Some(j) = journal.read().clone() else {
                return;
            };
            let Some(version_id) = j
                .effective_version
                .as_ref()
                .map(|v| v.id.clone())
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
                match journals::approve(&token, &id, &version_id).await {
                    Ok(v) => {
                        mutating.set(false);
                        save_banner
                            .set(Some(format!("Approved v{} ({})", v.version_number, v.state)));
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
                        error_msg.set(Some(err.message));
                    }
                }
            });
        }
    };

    let do_publish = {
        let id = journal_id.clone();
        move |_| {
            if mutating() {
                return;
            }
            let Some(j) = journal.read().clone() else {
                return;
            };
            let Some(version_id) = j
                .effective_version
                .as_ref()
                .map(|v| v.id.clone())
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
                match journals::publish(&token, &id, &version_id).await {
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
                        error_msg.set(Some(err.message));
                    }
                }
            });
        }
    };

    if loading() {
        return rsx! {
            div { class: "page-header",
                h1 { "Journal" }
            }
            p { class: "text-muted", "Loading journal..." }
        };
    }

    let Some(j) = journal_snapshot.clone() else {
        return rsx! {
            div { class: "page-header",
                h1 { "Journal" }
            }
            if let Some(msg) = error_msg() {
                div { class: "error-banner", "{msg}" }
            } else {
                p { class: "text-muted", "Journal not found." }
            }
        };
    };

    let is_published = j.is_published;
    let effective_state = j
        .effective_version
        .as_ref()
        .map(|v| v.state.clone())
        .unwrap_or_default();
    let effective_version_number = j
        .effective_version
        .as_ref()
        .map(|v| v.version_number)
        .unwrap_or(0);
    let can_approve = can_author && effective_state == "draft";
    let can_publish = can_author && effective_state == "approved";

    let attachment_props = AttachmentPanelProps {
        parent_type: ParentType::Journal,
        parent_id: j.id.clone(),
        can_upload: can_author,
        can_delete: can_author,
    };

    rsx! {
        div { class: "page-header library-header",
            div {
                h1 { "{j.title}" }
                p { class: "text-muted",
                    "Created {j.created_at} — updated {j.updated_at}"
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
                h2 { "{displayed_title}" }
                if let Some(n) = displayed_version_number {
                    span { class: "version-badge {displayed_state}", "v{n} {displayed_state}" }
                }
            }
            if displayed_body.is_empty() {
                p { class: "text-muted", "No content for this version yet." }
            } else {
                pre { class: "journal-body__content", "{displayed_body}" }
            }
        }

        if !versions.read().is_empty() {
            section { class: "version-history",
                h2 { "Version history" }
                ul { class: "version-list",
                    for v in versions.read().iter().cloned() {
                        VersionRow {
                            key: "{v.id}",
                            version: v.clone(),
                            on_select: move |ver: JournalVersionView| selected_version.set(Some(ver)),
                        }
                    }
                }
                if selected_version.read().is_some() {
                    button {
                        r#type: "button",
                        onclick: move |_| selected_version.set(None),
                        "Show effective version"
                    }
                }
            }
        }

        if can_author {
            section { class: "edit-panel",
                h2 { "Edit journal" }
                p { class: "text-muted",
                    "Saving creates a new draft version. Approved / published versions are immutable."
                }
                if let Some(msg) = edit_error() {
                    div { class: "error-banner", "{msg}" }
                }
                form { class: "edit-form", onsubmit: do_save,
                    label { r#for: "edit-title", "Title" }
                    input {
                        id: "edit-title",
                        r#type: "text",
                        value: "{edit_title()}",
                        oninput: move |evt| edit_title.set(evt.value()),
                    }
                    label { r#for: "edit-body", "Body" }
                    textarea {
                        id: "edit-body",
                        rows: "10",
                        value: "{edit_body()}",
                        oninput: move |evt| edit_body.set(evt.value()),
                        required: true,
                    }
                    label { r#for: "edit-summary", "Change summary (optional)" }
                    input {
                        id: "edit-summary",
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

        AttachmentPanel { ..attachment_props }
    }
}

#[derive(Props, Clone, PartialEq)]
struct VersionRowProps {
    version: JournalVersionView,
    on_select: EventHandler<JournalVersionView>,
}

#[component]
fn VersionRow(props: VersionRowProps) -> Element {
    let v = props.version.clone();
    let state = v.state.clone();
    let label_summary = v.change_summary.clone().unwrap_or_default();
    let created_at = v.created_at.clone();
    let version_number = v.version_number;
    let v_for_click = v.clone();

    rsx! {
        li { class: "version-row",
            div { class: "version-row__meta",
                span { class: "version-badge {state}", "v{version_number} {state}" }
                span { class: "text-muted", "{created_at}" }
            }
            if !label_summary.is_empty() {
                p { class: "version-row__summary", "{label_summary}" }
            }
            button {
                r#type: "button",
                class: "link-button",
                onclick: move |_| props.on_select.call(v_for_click.clone()),
                "View"
            }
        }
    }
}
