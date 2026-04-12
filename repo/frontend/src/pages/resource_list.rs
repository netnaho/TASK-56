//! Teaching-resource list page — browse and create teaching resources.

use dioxus::prelude::*;
use dioxus_router::prelude::*;

use crate::api::resources::{self, ResourceCreateInput, ResourceType, ResourceView};
use crate::router::AppRoute;
use crate::state::AuthState;
use crate::types::Role;

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
pub fn ResourceList() -> Element {
    let mut auth = use_context::<Signal<AuthState>>();
    let navigator = use_navigator();

    let mut items = use_signal(Vec::<ResourceView>::new);
    let mut loading = use_signal(|| true);
    let mut error_msg = use_signal(|| Option::<String>::None);

    let mut show_create = use_signal(|| false);
    let mut new_title = use_signal(String::new);
    let mut new_description = use_signal(String::new);
    let mut new_content_url = use_signal(String::new);
    let mut new_mime = use_signal(String::new);
    let mut new_change_summary = use_signal(String::new);
    let mut new_type = use_signal(|| "document".to_string());
    let mut creating = use_signal(|| false);
    let mut create_error = use_signal(|| Option::<String>::None);

    let mut reload_tick = use_signal(|| 0u32);

    let can_author = caller_can_author(&auth.read());

    use_effect(move || {
        let token = match auth.read().token.clone() {
            Some(t) => t,
            None => return,
        };
        let _tick = reload_tick.read();
        loading.set(true);
        error_msg.set(None);
        spawn(async move {
            match resources::list(&token, 50, 0).await {
                Ok(rows) => {
                    items.set(rows);
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
                    error_msg.set(Some(format!("Failed to load resources: {}", err.message)));
                }
            }
        });
    });

    let do_create = move |evt: FormEvent| {
        evt.prevent_default();
        if creating() {
            return;
        }
        let title = new_title().trim().to_string();
        if title.len() < 3 {
            create_error.set(Some("Title must be at least 3 characters.".to_string()));
            return;
        }
        let resource_type = ResourceType::from_snake(&new_type());
        let token = match auth.read().token.clone() {
            Some(t) => t,
            None => return,
        };
        let opt = |s: String| if s.trim().is_empty() { None } else { Some(s) };
        let input = ResourceCreateInput {
            title,
            resource_type,
            description: opt(new_description()),
            content_url: opt(new_content_url()),
            mime_type: opt(new_mime()),
            change_summary: opt(new_change_summary()),
        };
        creating.set(true);
        create_error.set(None);
        spawn(async move {
            match resources::create(&token, &input).await {
                Ok(_) => {
                    creating.set(false);
                    new_title.set(String::new());
                    new_description.set(String::new());
                    new_content_url.set(String::new());
                    new_mime.set(String::new());
                    new_change_summary.set(String::new());
                    show_create.set(false);
                    reload_tick.with_mut(|n| *n += 1);
                }
                Err(err) => {
                    creating.set(false);
                    if err.is_unauthorized() {
                        auth.set(AuthState::default());
                        AuthState::clear_storage();
                        navigator.push(AppRoute::Login {});
                        return;
                    }
                    create_error.set(Some(err.message));
                }
            }
        });
    };

    rsx! {
        div { class: "page-header library-header",
            div {
                h1 { "Teaching Resources" }
                p { class: "text-muted", "Browse documents, videos, and datasets." }
            }
            if can_author {
                button {
                    r#type: "button",
                    class: "primary-button",
                    onclick: move |_| {
                        let current = show_create();
                        show_create.set(!current);
                    },
                    if show_create() { "Cancel" } else { "New resource" }
                }
            }
        }

        if show_create() && can_author {
            form { class: "create-form", onsubmit: do_create,
                h2 { "Create a new resource" }
                if let Some(msg) = create_error() {
                    div { class: "error-banner", "{msg}" }
                }
                label { r#for: "new-res-title", "Title" }
                input {
                    id: "new-res-title",
                    r#type: "text",
                    value: "{new_title()}",
                    oninput: move |evt| new_title.set(evt.value()),
                    required: true,
                }
                label { r#for: "new-res-type", "Resource type" }
                select {
                    id: "new-res-type",
                    value: "{new_type()}",
                    oninput: move |evt| new_type.set(evt.value()),
                    for variant in ResourceType::all().iter() {
                        option {
                            value: "{variant.as_snake()}",
                            "{variant.label()}"
                        }
                    }
                }
                label { r#for: "new-res-desc", "Description (optional)" }
                textarea {
                    id: "new-res-desc",
                    rows: "3",
                    value: "{new_description()}",
                    oninput: move |evt| new_description.set(evt.value()),
                }
                label { r#for: "new-res-url", "Content URL (optional)" }
                input {
                    id: "new-res-url",
                    r#type: "url",
                    value: "{new_content_url()}",
                    oninput: move |evt| new_content_url.set(evt.value()),
                }
                label { r#for: "new-res-mime", "MIME type (optional)" }
                input {
                    id: "new-res-mime",
                    r#type: "text",
                    value: "{new_mime()}",
                    oninput: move |evt| new_mime.set(evt.value()),
                }
                label { r#for: "new-res-summary", "Change summary (optional)" }
                input {
                    id: "new-res-summary",
                    r#type: "text",
                    value: "{new_change_summary()}",
                    oninput: move |evt| new_change_summary.set(evt.value()),
                }
                button {
                    r#type: "submit",
                    class: "primary-button",
                    disabled: creating(),
                    if creating() { "Saving..." } else { "Create resource" }
                }
            }
        }

        if loading() {
            p { class: "text-muted", "Loading resources..." }
        } else if let Some(msg) = error_msg() {
            div { class: "error-banner", "{msg}" }
        } else if items.read().is_empty() {
            p { class: "text-muted", "No resources yet." }
        } else {
            div { class: "library-list",
                for resource in items.read().iter().cloned() {
                    ResourceCard { key: "{resource.id}", resource: resource.clone() }
                }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct ResourceCardProps {
    resource: ResourceView,
}

#[component]
fn ResourceCard(props: ResourceCardProps) -> Element {
    let r = props.resource.clone();
    let detail_route = AppRoute::ResourceDetail { id: r.id.clone() };

    let version_caption = match &r.effective_version {
        Some(v) => format!("v{} {}", v.version_number, v.state),
        None => "no versions".to_string(),
    };
    let type_label = r
        .resource_type
        .as_deref()
        .map(|t| ResourceType::from_snake(t).label())
        .unwrap_or("Resource");

    rsx! {
        div { class: "library-card",
            div { class: "library-card__title-row",
                h3 { class: "library-card__title", "{r.title}" }
                span { class: "resource-type-badge", "{type_label}" }
                if r.is_published {
                    span { class: "version-badge published", "Published" }
                } else {
                    span { class: "version-badge draft", "Draft" }
                }
            }
            p { class: "library-card__caption text-muted", "{version_caption}" }
            if let Some(desc) = r.effective_version.as_ref().and_then(|v| v.description.clone()) {
                p { class: "library-card__abstract", "{desc}" }
            }
            div { class: "library-card__footer",
                span { class: "text-muted library-card__updated", "Updated {r.updated_at}" }
                Link { class: "library-card__open", to: detail_route, "Open" }
            }
        }
    }
}
