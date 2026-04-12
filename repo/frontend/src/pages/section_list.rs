//! Section list page — view and create sections, optionally scoped to
//! a single parent course.
//!
//! The route `/courses/:id/sections` binds `id` to the course UUID used
//! as a filter; passing a blank id lists every section the caller can
//! see.

use dioxus::prelude::*;
use dioxus_router::prelude::*;

use crate::api::courses::{self, CourseView};
use crate::api::sections::{self, SectionCreateInput, SectionView};
use crate::router::AppRoute;
use crate::state::AuthState;
use crate::types::Role;

/// Returns `true` when the caller is permitted to create new sections.
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
pub fn SectionList(id: String) -> Element {
    let mut auth = use_context::<Signal<AuthState>>();
    let navigator = use_navigator();

    // `id` is the optional course filter — the /courses/:id/sections
    // route always provides one; an empty string means "all".
    let course_filter = id.clone();
    let course_filter_opt: Option<String> = if course_filter.is_empty() {
        None
    } else {
        Some(course_filter.clone())
    };

    let mut items = use_signal(Vec::<SectionView>::new);
    let mut courses_catalogue = use_signal(Vec::<CourseView>::new);
    let mut parent_course = use_signal(|| Option::<CourseView>::None);
    let mut loading = use_signal(|| true);
    let mut error_msg = use_signal(|| Option::<String>::None);

    let mut show_create = use_signal(|| false);
    let mut new_course_id = use_signal(|| course_filter.clone());
    let mut new_section_code = use_signal(String::new);
    let mut new_term = use_signal(|| "Fall".to_string());
    let mut new_year = use_signal(|| "2026".to_string());
    let mut new_capacity = use_signal(String::new);
    let mut new_instructor_id = use_signal(String::new);
    let mut new_location = use_signal(String::new);
    let mut new_schedule_note = use_signal(String::new);
    let mut new_notes = use_signal(String::new);
    let mut new_change_summary = use_signal(String::new);
    let mut creating = use_signal(|| false);
    let mut create_error = use_signal(|| Option::<String>::None);

    let mut reload_tick = use_signal(|| 0u32);

    let can_author = caller_can_author(&auth.read());

    {
        let filter_for_effect = course_filter_opt.clone();
        use_effect(move || {
            let token = match auth.read().token.clone() {
                Some(t) => t,
                None => return,
            };
            let _tick = reload_tick.read();
            let filter_clone = filter_for_effect.clone();
            loading.set(true);
            error_msg.set(None);
            spawn(async move {
                let rows_res =
                    sections::list(&token, filter_clone.as_deref(), 100, 0).await;
                let catalogue_res = courses::list(&token, None, 200, 0).await;
                let parent_res = if let Some(ref cid) = filter_clone {
                    Some(courses::get(&token, cid).await)
                } else {
                    None
                };
                loading.set(false);
                match rows_res {
                    Ok(rows) => items.set(rows),
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
                            "Failed to load sections: {}",
                            err.message
                        )));
                        return;
                    }
                }
                if let Ok(cat) = catalogue_res {
                    courses_catalogue.set(cat);
                }
                if let Some(Ok(c)) = parent_res {
                    parent_course.set(Some(c));
                }
            });
        });
    }

    let do_create = move |evt: FormEvent| {
        evt.prevent_default();
        if creating() {
            return;
        }
        let course_id_val = new_course_id().trim().to_string();
        if course_id_val.is_empty() {
            create_error.set(Some("Pick a course for this section.".to_string()));
            return;
        }
        let section_code = new_section_code().trim().to_string();
        if section_code.is_empty() {
            create_error.set(Some("Section code is required.".to_string()));
            return;
        }
        let term = new_term().trim().to_string();
        if term.is_empty() {
            create_error.set(Some("Term is required.".to_string()));
            return;
        }
        let year: i32 = match new_year().trim().parse() {
            Ok(v) => v,
            Err(_) => {
                create_error.set(Some("Year must be a number.".to_string()));
                return;
            }
        };
        let capacity: Option<i32> = {
            let s = new_capacity();
            let t = s.trim();
            if t.is_empty() {
                None
            } else {
                match t.parse() {
                    Ok(v) => Some(v),
                    Err(_) => {
                        create_error.set(Some("Capacity must be a number.".to_string()));
                        return;
                    }
                }
            }
        };
        let instructor_id = {
            let s = new_instructor_id().trim().to_string();
            if s.is_empty() {
                None
            } else {
                Some(s)
            }
        };
        let location = {
            let s = new_location();
            if s.trim().is_empty() {
                None
            } else {
                Some(s)
            }
        };
        let schedule_note = {
            let s = new_schedule_note();
            if s.trim().is_empty() {
                None
            } else {
                Some(s)
            }
        };
        let notes = {
            let s = new_notes();
            if s.trim().is_empty() {
                None
            } else {
                Some(s)
            }
        };
        let change_summary = {
            let s = new_change_summary();
            if s.trim().is_empty() {
                None
            } else {
                Some(s)
            }
        };
        let token = match auth.read().token.clone() {
            Some(t) => t,
            None => return,
        };
        let input = SectionCreateInput {
            course_id: course_id_val,
            section_code,
            term,
            year,
            capacity,
            instructor_id,
            location,
            schedule_note,
            notes,
            change_summary,
        };
        creating.set(true);
        create_error.set(None);
        spawn(async move {
            match sections::create(&token, &input).await {
                Ok(_) => {
                    creating.set(false);
                    new_section_code.set(String::new());
                    new_capacity.set(String::new());
                    new_instructor_id.set(String::new());
                    new_location.set(String::new());
                    new_schedule_note.set(String::new());
                    new_notes.set(String::new());
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
                    if err.is_forbidden() {
                        navigator.push(AppRoute::ForbiddenPage {});
                        return;
                    }
                    create_error.set(Some(err.message));
                }
            }
        });
    };

    let parent_label = {
        let p = parent_course.read().clone();
        p.map(|c| format!("{} — {}", c.code, c.title))
    };

    rsx! {
        div { class: "page-header library-header",
            div {
                h1 { "Sections" }
                if let Some(label) = parent_label.as_ref() {
                    p { class: "text-muted", "Filtered to course {label}" }
                } else {
                    p { class: "text-muted", "All sections visible to you." }
                }
            }
            if can_author {
                button {
                    r#type: "button",
                    class: "primary-button",
                    onclick: move |_| {
                        let current = show_create();
                        show_create.set(!current);
                    },
                    if show_create() { "Cancel" } else { "New section" }
                }
            }
        }

        if show_create() && can_author {
            form { class: "create-form", onsubmit: do_create,
                h2 { "Create a new section" }
                if let Some(msg) = create_error() {
                    div { class: "error-banner", "{msg}" }
                }
                label { r#for: "new-section-course", "Course" }
                select {
                    id: "new-section-course",
                    value: "{new_course_id()}",
                    oninput: move |evt| new_course_id.set(evt.value()),
                    option { value: "", "— select a course —" }
                    for cand in courses_catalogue.read().iter() {
                        option {
                            value: "{cand.id}",
                            "{cand.code} — {cand.title}"
                        }
                    }
                }
                label { r#for: "new-section-code", "Section code" }
                input {
                    id: "new-section-code",
                    r#type: "text",
                    value: "{new_section_code()}",
                    oninput: move |evt| new_section_code.set(evt.value()),
                    required: true,
                }
                div { class: "form-row",
                    div {
                        label { r#for: "new-section-term", "Term" }
                        input {
                            id: "new-section-term",
                            r#type: "text",
                            value: "{new_term()}",
                            oninput: move |evt| new_term.set(evt.value()),
                            required: true,
                        }
                    }
                    div {
                        label { r#for: "new-section-year", "Year" }
                        input {
                            id: "new-section-year",
                            r#type: "number",
                            value: "{new_year()}",
                            oninput: move |evt| new_year.set(evt.value()),
                            required: true,
                        }
                    }
                }
                div { class: "form-row",
                    div {
                        label { r#for: "new-section-capacity", "Capacity (optional)" }
                        input {
                            id: "new-section-capacity",
                            r#type: "number",
                            value: "{new_capacity()}",
                            oninput: move |evt| new_capacity.set(evt.value()),
                        }
                    }
                    div {
                        label { r#for: "new-section-instructor", "Instructor id (optional)" }
                        input {
                            id: "new-section-instructor",
                            r#type: "text",
                            value: "{new_instructor_id()}",
                            oninput: move |evt| new_instructor_id.set(evt.value()),
                        }
                    }
                }
                label { r#for: "new-section-location", "Location (optional)" }
                input {
                    id: "new-section-location",
                    r#type: "text",
                    value: "{new_location()}",
                    oninput: move |evt| new_location.set(evt.value()),
                }
                label { r#for: "new-section-schedule", "Schedule note (optional)" }
                input {
                    id: "new-section-schedule",
                    r#type: "text",
                    value: "{new_schedule_note()}",
                    oninput: move |evt| new_schedule_note.set(evt.value()),
                }
                label { r#for: "new-section-notes", "Notes (optional)" }
                textarea {
                    id: "new-section-notes",
                    rows: "2",
                    value: "{new_notes()}",
                    oninput: move |evt| new_notes.set(evt.value()),
                }
                label { r#for: "new-section-summary", "Change summary (optional)" }
                input {
                    id: "new-section-summary",
                    r#type: "text",
                    value: "{new_change_summary()}",
                    oninput: move |evt| new_change_summary.set(evt.value()),
                }
                button {
                    r#type: "submit",
                    class: "primary-button",
                    disabled: creating(),
                    if creating() { "Saving..." } else { "Create section" }
                }
            }
        }

        if loading() {
            p { class: "text-muted", "Loading sections..." }
        } else if let Some(msg) = error_msg() {
            div { class: "error-banner", "{msg}" }
        } else if items.read().is_empty() {
            p { class: "text-muted", "No sections yet." }
        } else {
            div { class: "library-list",
                for section_view in items.read().iter().cloned() {
                    SectionCard { key: "{section_view.id}", section: section_view.clone() }
                }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct SectionCardProps {
    section: SectionView,
}

#[component]
fn SectionCard(props: SectionCardProps) -> Element {
    let s = props.section.clone();
    let detail_route = AppRoute::SectionDetail { id: s.id.clone() };
    let version_caption = match &s.effective_version {
        Some(v) => format!("v{} {}", v.version_number, v.state),
        None => "no versions".to_string(),
    };
    let is_published = s
        .effective_version
        .as_ref()
        .map(|v| v.state == "published")
        .unwrap_or(false);
    let location = s
        .effective_version
        .as_ref()
        .and_then(|v| v.location.clone())
        .unwrap_or_default();
    let capacity = s
        .capacity
        .map(|c| c.to_string())
        .unwrap_or_else(|| "-".to_string());

    rsx! {
        div { class: "library-card",
            div { class: "library-card__title-row",
                h3 { class: "library-card__title",
                    "{s.course_code} · {s.section_code}"
                }
                if is_published {
                    span { class: "version-badge published", "Published" }
                } else {
                    span { class: "version-badge draft", "Draft" }
                }
            }
            p { class: "library-card__caption text-muted",
                "{s.term} {s.year} · capacity {capacity} · {version_caption}"
            }
            if !location.is_empty() {
                p { class: "library-card__abstract", "Location: {location}" }
            }
            div { class: "library-card__footer",
                span { class: "text-muted library-card__updated", "Updated {s.updated_at}" }
                Link { class: "library-card__open", to: detail_route, "Open" }
            }
        }
    }
}
