//! Course detail page — view, edit, approve, publish and manage
//! prerequisites for a single course, plus list its sections.

use dioxus::prelude::*;
use dioxus_router::prelude::*;

use crate::api::courses::{
    self, AddPrerequisiteInput, CourseEditInput, CourseVersionView, CourseView, PrerequisiteRef,
};
use crate::api::sections::{self, SectionView};
use crate::router::AppRoute;
use crate::state::AuthState;
use crate::types::Role;

#[derive(Props, Clone, PartialEq)]
pub struct CourseDetailProps {
    pub id: String,
}

/// Returns `true` when the caller can author courses (Admin or
/// DepartmentHead).
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
pub fn CourseDetail(props: CourseDetailProps) -> Element {
    let mut auth = use_context::<Signal<AuthState>>();
    let navigator = use_navigator();

    let course_id = props.id.clone();

    let mut course = use_signal(|| Option::<CourseView>::None);
    let mut versions = use_signal(Vec::<CourseVersionView>::new);
    let mut course_sections = use_signal(Vec::<SectionView>::new);
    let mut candidate_courses = use_signal(Vec::<CourseView>::new);
    let mut loading = use_signal(|| true);
    let mut error_msg = use_signal(|| Option::<String>::None);
    let mut save_banner = use_signal(|| Option::<String>::None);

    let mut edit_description = use_signal(String::new);
    let mut edit_syllabus = use_signal(String::new);
    let mut edit_credit = use_signal(|| "3".to_string());
    let mut edit_contact = use_signal(|| "3".to_string());
    let mut edit_summary = use_signal(String::new);
    let mut edit_error = use_signal(|| Option::<String>::None);
    let mut saving = use_signal(|| false);
    let mut mutating = use_signal(|| false);

    let mut new_prereq_id = use_signal(String::new);
    let mut new_prereq_grade = use_signal(String::new);
    let mut prereq_error = use_signal(|| Option::<String>::None);
    let mut prereq_busy = use_signal(|| false);

    let mut reload_tick = use_signal(|| 0u32);

    let can_author = caller_can_author(&auth.read());

    {
        let id_for_effect = course_id.clone();
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
                let course_res = courses::get(&token, &id).await;
                let versions_res = courses::list_versions(&token, &id).await;
                let sections_res = sections::list(&token, Some(&id), 100, 0).await;
                let candidates_res = courses::list(&token, None, 200, 0).await;
                loading.set(false);

                match course_res {
                    Ok(c) => {
                        if let Some(eff) = c.effective_version.clone() {
                            edit_description.set(eff.description.clone().unwrap_or_default());
                            edit_syllabus.set(eff.syllabus.clone().unwrap_or_default());
                            edit_credit.set(
                                eff.credit_hours
                                    .map(|v| v.to_string())
                                    .unwrap_or_else(|| "3".to_string()),
                            );
                            edit_contact.set(
                                eff.contact_hours
                                    .map(|v| v.to_string())
                                    .unwrap_or_else(|| "3".to_string()),
                            );
                            edit_summary.set(String::new());
                        }
                        course.set(Some(c));
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
                        error_msg.set(Some(format!("Failed to load course: {}", err.message)));
                        return;
                    }
                }

                match versions_res {
                    Ok(vs) => versions.set(vs),
                    Err(err) => {
                        if !err.is_forbidden() {
                            error_msg
                                .set(Some(format!("Failed to load versions: {}", err.message)));
                        }
                    }
                }

                match sections_res {
                    Ok(ss) => course_sections.set(ss),
                    Err(err) => {
                        if !err.is_forbidden() {
                            error_msg
                                .set(Some(format!("Failed to load sections: {}", err.message)));
                        }
                    }
                }

                match candidates_res {
                    Ok(cs) => candidate_courses.set(cs),
                    Err(_) => {
                        // Non-fatal — the dropdown just won't offer
                        // suggestions.
                    }
                }
            });
        });
    }

    let course_snapshot = course.read().clone();

    let do_save = {
        let id = course_id.clone();
        move |evt: FormEvent| {
            evt.prevent_default();
            if saving() {
                return;
            }
            let credit: f32 = match edit_credit().trim().parse() {
                Ok(v) => v,
                Err(_) => {
                    edit_error.set(Some("Credit hours must be a number.".to_string()));
                    return;
                }
            };
            let contact: f32 = match edit_contact().trim().parse() {
                Ok(v) => v,
                Err(_) => {
                    edit_error.set(Some("Contact hours must be a number.".to_string()));
                    return;
                }
            };
            let token = match auth.read().token.clone() {
                Some(t) => t,
                None => return,
            };
            let description_val = {
                let s = edit_description();
                if s.trim().is_empty() {
                    None
                } else {
                    Some(s)
                }
            };
            let syllabus_val = {
                let s = edit_syllabus();
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
            let input = CourseEditInput {
                description: description_val,
                syllabus: syllabus_val,
                credit_hours: credit,
                contact_hours: contact,
                change_summary: summary_val,
            };
            let id = id.clone();
            saving.set(true);
            edit_error.set(None);
            save_banner.set(None);
            spawn(async move {
                match courses::edit_draft(&token, &id, &input).await {
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
        let id = course_id.clone();
        move |_| {
            if mutating() {
                return;
            }
            let Some(c) = course.read().clone() else {
                return;
            };
            let Some(version_id) = c
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
                match courses::approve(&token, &id, &version_id).await {
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
        let id = course_id.clone();
        move |_| {
            if mutating() {
                return;
            }
            let Some(c) = course.read().clone() else {
                return;
            };
            let Some(version_id) = c
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
                match courses::publish(&token, &id, &version_id).await {
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

    let do_add_prereq = {
        let id = course_id.clone();
        move |evt: FormEvent| {
            evt.prevent_default();
            if prereq_busy() {
                return;
            }
            let prereq_id = new_prereq_id().trim().to_string();
            if prereq_id.is_empty() {
                prereq_error.set(Some("Pick a prerequisite course.".to_string()));
                return;
            }
            let min_grade = {
                let s = new_prereq_grade().trim().to_string();
                if s.is_empty() {
                    None
                } else {
                    Some(s)
                }
            };
            let token = match auth.read().token.clone() {
                Some(t) => t,
                None => return,
            };
            let input = AddPrerequisiteInput {
                prerequisite_course_id: prereq_id,
                min_grade,
            };
            let id = id.clone();
            prereq_busy.set(true);
            prereq_error.set(None);
            spawn(async move {
                match courses::add_prerequisite(&token, &id, &input).await {
                    Ok(()) => {
                        prereq_busy.set(false);
                        new_prereq_id.set(String::new());
                        new_prereq_grade.set(String::new());
                        reload_tick.with_mut(|n| *n += 1);
                    }
                    Err(err) => {
                        prereq_busy.set(false);
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
                        prereq_error.set(Some(err.message));
                    }
                }
            });
        }
    };

    if loading() {
        return rsx! {
            div { class: "page-header",
                h1 { "Course" }
            }
            p { class: "text-muted", "Loading course..." }
        };
    }

    let Some(c) = course_snapshot.clone() else {
        return rsx! {
            div { class: "page-header",
                h1 { "Course" }
            }
            if let Some(msg) = error_msg() {
                div { class: "error-banner", "{msg}" }
            } else {
                p { class: "text-muted", "Course not found." }
            }
        };
    };

    let effective_state = c
        .effective_version
        .as_ref()
        .map(|v| v.state.clone())
        .unwrap_or_default();
    let effective_version_number = c
        .effective_version
        .as_ref()
        .map(|v| v.version_number)
        .unwrap_or(0);
    let is_published = effective_state == "published";
    let can_approve = can_author && effective_state == "draft";
    let can_publish = can_author && effective_state == "approved";

    let department_label = c
        .department_id
        .clone()
        .unwrap_or_else(|| "-".to_string());

    let effective_body_description = c
        .effective_version
        .as_ref()
        .and_then(|v| v.description.clone())
        .unwrap_or_default();
    let effective_body_syllabus = c
        .effective_version
        .as_ref()
        .and_then(|v| v.syllabus.clone())
        .unwrap_or_default();
    let effective_credit_hours = c
        .effective_version
        .as_ref()
        .and_then(|v| v.credit_hours)
        .map(|h| h.to_string())
        .unwrap_or_else(|| "-".to_string());
    let effective_contact_hours = c
        .effective_version
        .as_ref()
        .and_then(|v| v.contact_hours)
        .map(|h| h.to_string())
        .unwrap_or_else(|| "-".to_string());

    let prereqs = c.prerequisites.clone();

    rsx! {
        div { class: "page-header library-header",
            div {
                h1 { "{c.code} — {c.title}" }
                p { class: "text-muted",
                    "Department {department_label} · Created {c.created_at} · Updated {c.updated_at}"
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

        div { class: "course-detail-layout",
            div { class: "course-detail-main",
                section { class: "journal-body",
                    header { class: "journal-body__header",
                        h2 { "Effective version" }
                        span { class: "version-badge {effective_state}",
                            "v{effective_version_number} {effective_state}"
                        }
                    }
                    dl { class: "course-meta",
                        dt { "Credit hours" }
                        dd { "{effective_credit_hours}" }
                        dt { "Contact hours" }
                        dd { "{effective_contact_hours}" }
                    }
                    h3 { "Description" }
                    if effective_body_description.is_empty() {
                        p { class: "text-muted", "No description." }
                    } else {
                        pre { class: "journal-body__content", "{effective_body_description}" }
                    }
                    h3 { "Syllabus" }
                    if effective_body_syllabus.is_empty() {
                        p { class: "text-muted", "No syllabus." }
                    } else {
                        pre { class: "journal-body__content", "{effective_body_syllabus}" }
                    }
                }

                if !versions.read().is_empty() {
                    section { class: "version-history",
                        h2 { "Version history" }
                        ul { class: "version-list",
                            for v in versions.read().iter().cloned() {
                                CourseVersionRow { key: "{v.id}", version: v.clone() }
                            }
                        }
                    }
                }

                if can_author {
                    section { class: "edit-panel",
                        h2 { "Edit course" }
                        p { class: "text-muted",
                            "Saving creates a new draft version. Approved / published versions are immutable."
                        }
                        if let Some(msg) = edit_error() {
                            div { class: "error-banner", "{msg}" }
                        }
                        form { class: "edit-form", onsubmit: do_save,
                            label { r#for: "edit-description", "Description" }
                            textarea {
                                id: "edit-description",
                                rows: "4",
                                value: "{edit_description()}",
                                oninput: move |evt| edit_description.set(evt.value()),
                            }
                            label { r#for: "edit-syllabus", "Syllabus" }
                            textarea {
                                id: "edit-syllabus",
                                rows: "8",
                                value: "{edit_syllabus()}",
                                oninput: move |evt| edit_syllabus.set(evt.value()),
                            }
                            div { class: "form-row",
                                div {
                                    label { r#for: "edit-credit", "Credit hours" }
                                    input {
                                        id: "edit-credit",
                                        r#type: "number",
                                        step: "0.5",
                                        value: "{edit_credit()}",
                                        oninput: move |evt| edit_credit.set(evt.value()),
                                        required: true,
                                    }
                                }
                                div {
                                    label { r#for: "edit-contact", "Contact hours" }
                                    input {
                                        id: "edit-contact",
                                        r#type: "number",
                                        step: "0.5",
                                        value: "{edit_contact()}",
                                        oninput: move |evt| edit_contact.set(evt.value()),
                                        required: true,
                                    }
                                }
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
            }

            aside { class: "course-detail-sidebar",
                section { class: "prereq-panel",
                    h2 { "Prerequisites" }
                    if prereqs.is_empty() {
                        p { class: "text-muted", "No prerequisites." }
                    } else {
                        ul { class: "prereq-list",
                            for prereq in prereqs.iter().cloned() {
                                PrereqChip {
                                    key: "{prereq.prerequisite_course_id}",
                                    course_id: c.id.clone(),
                                    prereq: prereq.clone(),
                                    can_edit: can_author,
                                    on_changed: move |_| reload_tick.with_mut(|n| *n += 1),
                                }
                            }
                        }
                    }
                    if can_author {
                        if let Some(msg) = prereq_error() {
                            div { class: "error-banner", "{msg}" }
                        }
                        form { class: "prereq-add-form", onsubmit: do_add_prereq,
                            label { r#for: "add-prereq", "Add prerequisite" }
                            select {
                                id: "add-prereq",
                                value: "{new_prereq_id()}",
                                oninput: move |evt| new_prereq_id.set(evt.value()),
                                option { value: "", "— select a course —" }
                                for cand in candidate_courses.read().iter() {
                                    if cand.id != c.id {
                                        option {
                                            value: "{cand.id}",
                                            "{cand.code} — {cand.title}"
                                        }
                                    }
                                }
                            }
                            label { r#for: "add-prereq-grade", "Min grade (optional)" }
                            input {
                                id: "add-prereq-grade",
                                r#type: "text",
                                value: "{new_prereq_grade()}",
                                oninput: move |evt| new_prereq_grade.set(evt.value()),
                            }
                            button {
                                r#type: "submit",
                                class: "primary-button",
                                disabled: prereq_busy(),
                                if prereq_busy() { "Adding..." } else { "Add prerequisite" }
                            }
                        }
                    }
                }

                section { class: "sections-panel",
                    h2 { "Sections" }
                    if course_sections.read().is_empty() {
                        p { class: "text-muted", "No sections yet." }
                    } else {
                        ul { class: "section-list",
                            for s in course_sections.read().iter().cloned() {
                                SectionLinkRow { key: "{s.id}", section: s.clone() }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct CourseVersionRowProps {
    version: CourseVersionView,
}

#[component]
fn CourseVersionRow(props: CourseVersionRowProps) -> Element {
    let v = props.version.clone();
    let state = v.state.clone();
    let summary = v.change_summary.clone().unwrap_or_default();
    let created_at = v.created_at.clone();
    let version_number = v.version_number;

    rsx! {
        li { class: "version-row",
            div { class: "version-row__meta",
                span { class: "version-badge {state}", "v{version_number} {state}" }
                span { class: "text-muted", "{created_at}" }
            }
            if !summary.is_empty() {
                p { class: "version-row__summary", "{summary}" }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct PrereqChipProps {
    course_id: String,
    prereq: PrerequisiteRef,
    can_edit: bool,
    on_changed: EventHandler<()>,
}

#[component]
fn PrereqChip(props: PrereqChipProps) -> Element {
    let mut auth = use_context::<Signal<AuthState>>();
    let navigator = use_navigator();
    let mut busy = use_signal(|| false);

    let course_id = props.course_id.clone();
    let prereq = props.prereq.clone();
    let prereq_id = prereq.prerequisite_course_id.clone();
    let code = prereq.prerequisite_code.clone();
    let min_grade = prereq.min_grade.clone().unwrap_or_default();
    let can_edit = props.can_edit;
    let on_changed = props.on_changed;

    let do_remove = {
        let prereq_id = prereq_id.clone();
        move |_| {
            if busy() {
                return;
            }
            let token = match auth.read().token.clone() {
                Some(t) => t,
                None => return,
            };
            let course_id = course_id.clone();
            let prereq_id = prereq_id.clone();
            busy.set(true);
            spawn(async move {
                match courses::remove_prerequisite(&token, &course_id, &prereq_id).await {
                    Ok(()) => {
                        busy.set(false);
                        on_changed.call(());
                    }
                    Err(err) => {
                        busy.set(false);
                        if err.is_unauthorized() {
                            auth.set(AuthState::default());
                            AuthState::clear_storage();
                            navigator.push(AppRoute::Login {});
                            return;
                        }
                        if err.is_forbidden() {
                            navigator.push(AppRoute::ForbiddenPage {});
                        }
                    }
                }
            });
        }
    };

    rsx! {
        li { class: "prereq-chip",
            span { class: "prereq-chip__code", "{code}" }
            if !min_grade.is_empty() {
                span { class: "prereq-chip__grade", "≥ {min_grade}" }
            }
            if can_edit {
                button {
                    r#type: "button",
                    class: "link-button",
                    disabled: busy(),
                    onclick: do_remove,
                    if busy() { "…" } else { "remove" }
                }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct SectionLinkRowProps {
    section: SectionView,
}

#[component]
fn SectionLinkRow(props: SectionLinkRowProps) -> Element {
    let s = props.section.clone();
    let route = AppRoute::SectionDetail { id: s.id.clone() };
    let state = s
        .effective_version
        .as_ref()
        .map(|v| v.state.clone())
        .unwrap_or_else(|| "-".to_string());

    rsx! {
        li { class: "section-link-row",
            div { class: "section-link-row__meta",
                strong { "{s.section_code}" }
                span { class: "text-muted", " · {s.term} {s.year}" }
                span { class: "version-badge {state}", "{state}" }
            }
            Link { class: "link-button", to: route, "Open" }
        }
    }
}
