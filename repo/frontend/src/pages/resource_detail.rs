//! Teaching-resource detail page — view, edit, approve, and publish a
//! single teaching resource, plus manage its attachments.

use dioxus::prelude::*;
use dioxus_router::prelude::*;

use crate::api::attachments::ParentType;
use crate::api::resources::{self, ResourceEditInput, ResourceVersionView, ResourceView};
use crate::components::attachment_panel::{AttachmentPanel, AttachmentPanelProps};
use crate::router::AppRoute;
use crate::state::AuthState;
use crate::types::Role;

#[derive(Props, Clone, PartialEq)]
pub struct ResourceDetailProps {
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
pub fn ResourceDetail(props: ResourceDetailProps) -> Element {
    let mut auth = use_context::<Signal<AuthState>>();
    let navigator = use_navigator();

    let resource_id = props.id.clone();

    let mut resource = use_signal(|| Option::<ResourceView>::None);
    let mut versions = use_signal(Vec::<ResourceVersionView>::new);
    let mut selected_version = use_signal(|| Option::<ResourceVersionView>::None);
    let mut loading = use_signal(|| true);
    let mut error_msg = use_signal(|| Option::<String>::None);
    let mut save_banner = use_signal(|| Option::<String>::None);

    let mut edit_title = use_signal(String::new);
    let mut edit_description = use_signal(String::new);
    let mut edit_content_url = use_signal(String::new);
    let mut edit_mime = use_signal(String::new);
    let mut edit_summary = use_signal(String::new);
    let mut edit_error = use_signal(|| Option::<String>::None);
    let mut saving = use_signal(|| false);
    let mut mutating = use_signal(|| false);

    let mut reload_tick = use_signal(|| 0u32);

    let can_author = caller_can_author(&auth.read());

    {
        let id_for_effect = resource_id.clone();
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
                let res = resources::get(&token, &id).await;
                let vers = resources::list_versions(&token, &id).await;
                loading.set(false);
                match res {
                    Ok(r) => {
                        edit_title.set(r.title.clone());
                        if let Some(v) = r.effective_version.clone() {
                            edit_description.set(v.description.unwrap_or_default());
                            edit_content_url.set(v.content_url.unwrap_or_default());
                            edit_mime.set(v.mime_type.unwrap_or_default());
                        } else {
                            edit_description.set(String::new());
                            edit_content_url.set(String::new());
                            edit_mime.set(String::new());
                        }
                        edit_summary.set(String::new());
                        resource.set(Some(r));
                    }
                    Err(err) => {
                        if err.is_unauthorized() {
                            auth.set(AuthState::default());
                            AuthState::clear_storage();
                            navigator.push(AppRoute::Login {});
                            return;
                        }
                        error_msg.set(Some(format!("Failed to load resource: {}", err.message)));
                        return;
                    }
                }
                match vers {
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

    let resource_snapshot = resource.read().clone();

    let (displayed_title, displayed_description, displayed_content_url, displayed_state, displayed_version_number) = {
        let selected = selected_version.read().clone();
        if let Some(v) = selected {
            (
                v.title.unwrap_or_default(),
                v.description.unwrap_or_default(),
                v.content_url.unwrap_or_default(),
                v.state,
                Some(v.version_number),
            )
        } else if let Some(r) = resource_snapshot.as_ref() {
            let eff = r.effective_version.clone();
            (
                r.title.clone(),
                eff.as_ref()
                    .and_then(|v| v.description.clone())
                    .unwrap_or_default(),
                eff.as_ref()
                    .and_then(|v| v.content_url.clone())
                    .unwrap_or_default(),
                eff.as_ref()
                    .map(|v| v.state.clone())
                    .unwrap_or_else(|| "-".to_string()),
                eff.as_ref().map(|v| v.version_number),
            )
        } else {
            (String::new(), String::new(), String::new(), "-".to_string(), None)
        }
    };

    let do_save = {
        let id = resource_id.clone();
        move |evt: FormEvent| {
            evt.prevent_default();
            if saving() {
                return;
            }
            let title = edit_title().trim().to_string();
            if !title.is_empty() && title.len() < 3 {
                edit_error.set(Some("Title must be at least 3 characters.".to_string()));
                return;
            }
            let opt = |s: String| if s.trim().is_empty() { None } else { Some(s) };
            let input = ResourceEditInput {
                title: if title.is_empty() { None } else { Some(title) },
                description: opt(edit_description()),
                content_url: opt(edit_content_url()),
                mime_type: opt(edit_mime()),
                change_summary: opt(edit_summary()),
            };
            let token = match auth.read().token.clone() {
                Some(t) => t,
                None => return,
            };
            let id = id.clone();
            saving.set(true);
            edit_error.set(None);
            save_banner.set(None);
            spawn(async move {
                match resources::edit(&token, &id, &input).await {
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
        let id = resource_id.clone();
        move |_| {
            if mutating() {
                return;
            }
            let Some(r) = resource.read().clone() else {
                return;
            };
            let Some(version_id) = r.effective_version.as_ref().map(|v| v.id.clone()) else {
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
                match resources::approve(&token, &id, &version_id).await {
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
        let id = resource_id.clone();
        move |_| {
            if mutating() {
                return;
            }
            let Some(r) = resource.read().clone() else {
                return;
            };
            let Some(version_id) = r.effective_version.as_ref().map(|v| v.id.clone()) else {
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
                match resources::publish(&token, &id, &version_id).await {
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
                h1 { "Teaching Resource" }
            }
            p { class: "text-muted", "Loading resource..." }
        };
    }

    let Some(r) = resource_snapshot.clone() else {
        return rsx! {
            div { class: "page-header",
                h1 { "Teaching Resource" }
            }
            if let Some(msg) = error_msg() {
                div { class: "error-banner", "{msg}" }
            } else {
                p { class: "text-muted", "Resource not found." }
            }
        };
    };

    let is_published = r.is_published;
    let effective_state = r
        .effective_version
        .as_ref()
        .map(|v| v.state.clone())
        .unwrap_or_default();
    let effective_version_number = r
        .effective_version
        .as_ref()
        .map(|v| v.version_number)
        .unwrap_or(0);
    let can_approve = can_author && effective_state == "draft";
    let can_publish = can_author && effective_state == "approved";

    let attachment_props = AttachmentPanelProps {
        parent_type: ParentType::TeachingResource,
        parent_id: r.id.clone(),
        can_upload: can_author,
        can_delete: can_author,
    };

    rsx! {
        div { class: "page-header library-header",
            div {
                h1 { "{r.title}" }
                p { class: "text-muted",
                    "Created {r.created_at} — updated {r.updated_at}"
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
            if !displayed_description.is_empty() {
                p { class: "journal-body__content", "{displayed_description}" }
            } else {
                p { class: "text-muted", "No description for this version yet." }
            }
            if !displayed_content_url.is_empty() {
                p {
                    class: "text-muted",
                    "Content URL: "
                    a {
                        href: "{displayed_content_url}",
                        target: "_blank",
                        rel: "noopener noreferrer",
                        "{displayed_content_url}"
                    }
                }
            }
        }

        if !versions.read().is_empty() {
            section { class: "version-history",
                h2 { "Version history" }
                ul { class: "version-list",
                    for v in versions.read().iter().cloned() {
                        ResourceVersionRow {
                            key: "{v.id}",
                            version: v.clone(),
                            on_select: move |ver: ResourceVersionView| selected_version.set(Some(ver)),
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
                h2 { "Edit resource" }
                p { class: "text-muted",
                    "Saving creates a new draft version. Approved / published versions are immutable."
                }
                if let Some(msg) = edit_error() {
                    div { class: "error-banner", "{msg}" }
                }
                form { class: "edit-form", onsubmit: do_save,
                    label { r#for: "edit-res-title", "Title" }
                    input {
                        id: "edit-res-title",
                        r#type: "text",
                        value: "{edit_title()}",
                        oninput: move |evt| edit_title.set(evt.value()),
                    }
                    label { r#for: "edit-res-desc", "Description" }
                    textarea {
                        id: "edit-res-desc",
                        rows: "6",
                        value: "{edit_description()}",
                        oninput: move |evt| edit_description.set(evt.value()),
                    }
                    label { r#for: "edit-res-url", "Content URL" }
                    input {
                        id: "edit-res-url",
                        r#type: "url",
                        value: "{edit_content_url()}",
                        oninput: move |evt| edit_content_url.set(evt.value()),
                    }
                    label { r#for: "edit-res-mime", "MIME type" }
                    input {
                        id: "edit-res-mime",
                        r#type: "text",
                        value: "{edit_mime()}",
                        oninput: move |evt| edit_mime.set(evt.value()),
                    }
                    label { r#for: "edit-res-summary", "Change summary (optional)" }
                    input {
                        id: "edit-res-summary",
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
struct ResourceVersionRowProps {
    version: ResourceVersionView,
    on_select: EventHandler<ResourceVersionView>,
}

#[component]
fn ResourceVersionRow(props: ResourceVersionRowProps) -> Element {
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
