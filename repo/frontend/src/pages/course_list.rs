//! Course list page — browse and create courses.
//!
//! Mirrors the journal list in shape: a scrollable grid of cards with
//! an inline "New course" form for DepartmentHead / Admin users. Each
//! card surfaces the current version state, prerequisites count and
//! links to the detail page for editing / publishing.

use dioxus::prelude::*;
use dioxus_router::prelude::*;

use crate::api::courses::{self, CourseCreateInput, CourseView};
use crate::router::AppRoute;
use crate::state::AuthState;
use crate::types::Role;

/// Returns `true` when the caller can create / manage courses.
/// DepartmentHead and Admin may author course records.
fn caller_can_author(auth: &AuthState) -> bool {
    auth.user
        .as_ref()
        .map(|u| {
            u.roles
                .iter()
                .any(|r| matches!(r, Role::Admin | Role::DepartmentHead))
        })
        .unwrap_or(false)
}

#[component]
pub fn CourseList() -> Element {
    let mut auth = use_context::<Signal<AuthState>>();
    let navigator = use_navigator();

    let mut items = use_signal(Vec::<CourseView>::new);
    let mut loading = use_signal(|| true);
    let mut error_msg = use_signal(|| Option::<String>::None);

    let mut show_create = use_signal(|| false);
    let mut new_code = use_signal(String::new);
    let mut new_title = use_signal(String::new);
    let mut new_description = use_signal(String::new);
    let mut new_syllabus = use_signal(String::new);
    let mut new_credit_hours = use_signal(|| "3".to_string());
    let mut new_contact_hours = use_signal(|| "3".to_string());
    let mut new_change_summary = use_signal(String::new);
    let mut creating = use_signal(|| false);
    let mut create_error = use_signal(|| Option::<String>::None);

    let mut reload_tick = use_signal(|| 0u32);

    let snapshot = auth.read().clone();
    let can_author = caller_can_author(&snapshot);
    let department_scope = snapshot
        .user
        .as_ref()
        .and_then(|u| u.department_id.clone());
    let role_label = snapshot
        .primary_role()
        .map(|r| r.to_string())
        .unwrap_or_else(|| "Viewer".to_string());
    let header_caption = match department_scope.as_ref() {
        Some(dept) => format!(
            "Browse the course catalogue. Scoped to department {}. Signed in as {}.",
            dept, role_label
        ),
        None => format!(
            "Browse the course catalogue. Signed in as {}.",
            role_label
        ),
    };

    use_effect(move || {
        let token = match auth.read().token.clone() {
            Some(t) => t,
            None => return,
        };
        let _tick = reload_tick.read();
        loading.set(true);
        error_msg.set(None);
        spawn(async move {
            match courses::list(&token, None, 100, 0).await {
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
                    if err.is_forbidden() {
                        navigator.push(AppRoute::ForbiddenPage {});
                        return;
                    }
                    error_msg.set(Some(format!("Failed to load courses: {}", err.message)));
                }
            }
        });
    });

    let do_create = move |evt: FormEvent| {
        evt.prevent_default();
        if creating() {
            return;
        }
        let code = new_code().trim().to_string();
        let title = new_title().trim().to_string();
        if code.is_empty() {
            create_error.set(Some("Course code is required.".to_string()));
            return;
        }
        if title.len() < 3 {
            create_error.set(Some("Title must be at least 3 characters.".to_string()));
            return;
        }
        let credit_hours: f32 = match new_credit_hours().trim().parse() {
            Ok(v) => v,
            Err(_) => {
                create_error.set(Some("Credit hours must be a number.".to_string()));
                return;
            }
        };
        let contact_hours: f32 = match new_contact_hours().trim().parse() {
            Ok(v) => v,
            Err(_) => {
                create_error.set(Some("Contact hours must be a number.".to_string()));
                return;
            }
        };
        let token = match auth.read().token.clone() {
            Some(t) => t,
            None => return,
        };
        let dept_for_create = auth
            .read()
            .user
            .as_ref()
            .and_then(|u| u.department_id.clone());
        let description_val = {
            let s = new_description();
            if s.trim().is_empty() {
                None
            } else {
                Some(s)
            }
        };
        let syllabus_val = {
            let s = new_syllabus();
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
        let input = CourseCreateInput {
            code,
            title,
            department_id: dept_for_create,
            description: description_val,
            syllabus: syllabus_val,
            credit_hours,
            contact_hours,
            change_summary: change_val,
        };
        creating.set(true);
        create_error.set(None);
        spawn(async move {
            match courses::create(&token, &input).await {
                Ok(_) => {
                    creating.set(false);
                    new_code.set(String::new());
                    new_title.set(String::new());
                    new_description.set(String::new());
                    new_syllabus.set(String::new());
                    new_credit_hours.set("3".to_string());
                    new_contact_hours.set("3".to_string());
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

    rsx! {
        div { class: "page-header library-header",
            div {
                h1 { "Courses" }
                p { class: "text-muted", "{header_caption}" }
            }
            if can_author {
                button {
                    r#type: "button",
                    class: "primary-button",
                    onclick: move |_| {
                        let current = show_create();
                        show_create.set(!current);
                    },
                    if show_create() { "Cancel" } else { "New course" }
                }
            }
        }

        if show_create() && can_author {
            form { class: "create-form", onsubmit: do_create,
                h2 { "Create a new course" }
                if let Some(msg) = create_error() {
                    div { class: "error-banner", "{msg}" }
                }
                label { r#for: "new-course-code", "Course code" }
                input {
                    id: "new-course-code",
                    r#type: "text",
                    value: "{new_code()}",
                    oninput: move |evt| new_code.set(evt.value()),
                    required: true,
                }
                label { r#for: "new-course-title", "Title" }
                input {
                    id: "new-course-title",
                    r#type: "text",
                    value: "{new_title()}",
                    oninput: move |evt| new_title.set(evt.value()),
                    required: true,
                }
                label { r#for: "new-course-description", "Description (optional)" }
                textarea {
                    id: "new-course-description",
                    rows: "3",
                    value: "{new_description()}",
                    oninput: move |evt| new_description.set(evt.value()),
                }
                label { r#for: "new-course-syllabus", "Syllabus (optional)" }
                textarea {
                    id: "new-course-syllabus",
                    rows: "4",
                    value: "{new_syllabus()}",
                    oninput: move |evt| new_syllabus.set(evt.value()),
                }
                div { class: "form-row",
                    div {
                        label { r#for: "new-course-credit", "Credit hours" }
                        input {
                            id: "new-course-credit",
                            r#type: "number",
                            step: "0.5",
                            value: "{new_credit_hours()}",
                            oninput: move |evt| new_credit_hours.set(evt.value()),
                            required: true,
                        }
                    }
                    div {
                        label { r#for: "new-course-contact", "Contact hours" }
                        input {
                            id: "new-course-contact",
                            r#type: "number",
                            step: "0.5",
                            value: "{new_contact_hours()}",
                            oninput: move |evt| new_contact_hours.set(evt.value()),
                            required: true,
                        }
                    }
                }
                label { r#for: "new-course-summary", "Change summary (optional)" }
                input {
                    id: "new-course-summary",
                    r#type: "text",
                    value: "{new_change_summary()}",
                    oninput: move |evt| new_change_summary.set(evt.value()),
                }
                button {
                    r#type: "submit",
                    class: "primary-button",
                    disabled: creating(),
                    if creating() { "Saving..." } else { "Create course" }
                }
            }
        }

        if loading() {
            p { class: "text-muted", "Loading courses..." }
        } else if let Some(msg) = error_msg() {
            div { class: "error-banner", "{msg}" }
        } else if items.read().is_empty() {
            p { class: "text-muted", "No courses yet." }
        } else {
            div { class: "library-list",
                for course in items.read().iter().cloned() {
                    CourseCard { key: "{course.id}", course: course.clone() }
                }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct CourseCardProps {
    course: CourseView,
}

#[component]
fn CourseCard(props: CourseCardProps) -> Element {
    let c = props.course.clone();
    let detail_route = AppRoute::CourseDetail { id: c.id.clone() };

    let version_caption = match &c.effective_version {
        Some(v) => format!("v{} {}", v.version_number, v.state),
        None => "no versions".to_string(),
    };
    let is_published = c
        .effective_version
        .as_ref()
        .map(|v| v.state == "published")
        .unwrap_or(false);

    let prereq_count = c.prerequisites.len();
    let prereq_caption = if prereq_count == 1 {
        "1 prereq".to_string()
    } else {
        format!("{} prereqs", prereq_count)
    };
    let description = c
        .effective_version
        .as_ref()
        .and_then(|v| v.description.clone())
        .unwrap_or_default();
    let credit_caption = c
        .effective_version
        .as_ref()
        .and_then(|v| v.credit_hours)
        .map(|h| format!("{} credit hrs", h))
        .unwrap_or_else(|| "-".to_string());

    rsx! {
        div { class: "library-card",
            div { class: "library-card__title-row",
                h3 { class: "library-card__title", "{c.code} — {c.title}" }
                if is_published {
                    span { class: "version-badge published", "Published" }
                } else {
                    span { class: "version-badge draft", "Draft" }
                }
            }
            p { class: "library-card__caption text-muted",
                "{version_caption} · {credit_caption} · {prereq_caption}"
            }
            if !description.is_empty() {
                p { class: "library-card__abstract", "{description}" }
            }
            div { class: "library-card__footer",
                span { class: "text-muted library-card__updated", "Updated {c.updated_at}" }
                Link { class: "library-card__open", to: detail_route, "Open" }
            }
        }
    }
}
