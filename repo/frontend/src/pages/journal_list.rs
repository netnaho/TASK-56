//! Journal list page — browse and create journals.
//!
//! Renders the journal catalogue as a grid of cards, each linking to a
//! detail view. Editor-level users additionally see an inline
//! "New journal" form.

use dioxus::prelude::*;
use dioxus_router::prelude::*;

use crate::api::journals::{self, JournalCreateInput, JournalView};
use crate::router::AppRoute;
use crate::state::AuthState;
use crate::types::Role;

/// Returns `true` when the caller has editor privileges (admin or
/// librarian) and should therefore see the "New journal" UI.
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
pub fn JournalList() -> Element {
    let mut auth = use_context::<Signal<AuthState>>();
    let navigator = use_navigator();

    let mut items = use_signal(Vec::<JournalView>::new);
    let mut loading = use_signal(|| true);
    let mut error_msg = use_signal(|| Option::<String>::None);

    let mut show_create = use_signal(|| false);
    let mut new_title = use_signal(String::new);
    let mut new_abstract = use_signal(String::new);
    let mut new_body = use_signal(String::new);
    let mut new_change_summary = use_signal(String::new);
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
            match journals::list(&token, 50, 0).await {
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
                    error_msg.set(Some(format!("Failed to load journals: {}", err.message)));
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
        let body = new_body();
        if title.len() < 3 {
            create_error.set(Some("Title must be at least 3 characters.".to_string()));
            return;
        }
        if body.trim().is_empty() {
            create_error.set(Some("Body must not be empty.".to_string()));
            return;
        }
        let token = match auth.read().token.clone() {
            Some(t) => t,
            None => return,
        };
        let abstract_val = {
            let s = new_abstract();
            if s.trim().is_empty() {
                None
            } else {
                Some(s)
            }
        };
        let change_val = {
            let s = new_change_summary();
            if s.trim().is_empty() {
                None
            } else {
                Some(s)
            }
        };
        let input = JournalCreateInput {
            title,
            abstract_text: abstract_val,
            body,
            change_summary: change_val,
        };
        creating.set(true);
        create_error.set(None);
        spawn(async move {
            match journals::create(&token, &input).await {
                Ok(_) => {
                    creating.set(false);
                    new_title.set(String::new());
                    new_abstract.set(String::new());
                    new_body.set(String::new());
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
                h1 { "Journals" }
                p { class: "text-muted", "Browse the journal catalogue." }
            }
            if can_author {
                button {
                    r#type: "button",
                    class: "primary-button",
                    onclick: move |_| {
                        let current = show_create();
                        show_create.set(!current);
                    },
                    if show_create() { "Cancel" } else { "New journal" }
                }
            }
        }

        if show_create() && can_author {
            form { class: "create-form", onsubmit: do_create,
                h2 { "Create a new journal" }
                if let Some(msg) = create_error() {
                    div { class: "error-banner", "{msg}" }
                }
                label { r#for: "new-journal-title", "Title" }
                input {
                    id: "new-journal-title",
                    r#type: "text",
                    value: "{new_title()}",
                    oninput: move |evt| new_title.set(evt.value()),
                    required: true,
                }
                label { r#for: "new-journal-abstract", "Abstract (optional)" }
                textarea {
                    id: "new-journal-abstract",
                    rows: "2",
                    value: "{new_abstract()}",
                    oninput: move |evt| new_abstract.set(evt.value()),
                }
                label { r#for: "new-journal-body", "Body" }
                textarea {
                    id: "new-journal-body",
                    rows: "6",
                    value: "{new_body()}",
                    oninput: move |evt| new_body.set(evt.value()),
                    required: true,
                }
                label { r#for: "new-journal-summary", "Change summary (optional)" }
                input {
                    id: "new-journal-summary",
                    r#type: "text",
                    value: "{new_change_summary()}",
                    oninput: move |evt| new_change_summary.set(evt.value()),
                }
                button {
                    r#type: "submit",
                    class: "primary-button",
                    disabled: creating(),
                    if creating() { "Saving..." } else { "Create journal" }
                }
            }
        }

        if loading() {
            p { class: "text-muted", "Loading journals..." }
        } else if let Some(msg) = error_msg() {
            div { class: "error-banner", "{msg}" }
        } else if items.read().is_empty() {
            p { class: "text-muted", "No journals yet." }
        } else {
            div { class: "library-list",
                for journal in items.read().iter().cloned() {
                    JournalCard { key: "{journal.id}", journal: journal.clone() }
                }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct JournalCardProps {
    journal: JournalView,
}

#[component]
fn JournalCard(props: JournalCardProps) -> Element {
    let j = props.journal.clone();
    let detail_route = AppRoute::JournalDetail { id: j.id.clone() };

    let version_caption = match &j.effective_version {
        Some(v) => format!("v{} {}", v.version_number, v.state),
        None => "no versions".to_string(),
    };

    rsx! {
        div { class: "library-card",
            div { class: "library-card__title-row",
                h3 { class: "library-card__title", "{j.title}" }
                if j.is_published {
                    span { class: "version-badge published", "Published" }
                } else {
                    span { class: "version-badge draft", "Draft" }
                }
            }
            p { class: "library-card__caption text-muted", "{version_caption}" }
            if let Some(abs) = j.abstract_text.as_ref() {
                p { class: "library-card__abstract", "{abs}" }
            }
            div { class: "library-card__footer",
                span { class: "text-muted library-card__updated", "Updated {j.updated_at}" }
                Link { class: "library-card__open", to: detail_route, "Open" }
            }
        }
    }
}
