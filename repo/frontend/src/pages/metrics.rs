//! Phase 5 metric-definitions catalog.
//!
//! Lists every metric definition and lets authors draft new ones or
//! drill into an existing metric to edit its formula, approve drafts,
//! and publish approved versions.

use dioxus::prelude::*;
use dioxus_router::prelude::*;

use crate::api::metrics::{
    self, LineageRef, MetricCreateInput, MetricDefinitionView, MetricVersionView,
};
use crate::router::AppRoute;
use crate::state::AuthState;
use crate::types::Role;

fn caller_is_admin(auth: &AuthState) -> bool {
    auth.user
        .as_ref()
        .map(|u| u.roles.iter().any(|r| matches!(r, Role::Admin)))
        .unwrap_or(false)
}

fn caller_can_author(auth: &AuthState) -> bool {
    auth.user
        .as_ref()
        .map(|u| {
            u.roles.iter().any(|r| {
                matches!(
                    r,
                    Role::Admin | Role::Librarian | Role::DepartmentHead
                )
            })
        })
        .unwrap_or(false)
}

#[component]
pub fn Metrics() -> Element {
    let mut auth = use_context::<Signal<AuthState>>();
    let navigator = use_navigator();

    let mut items = use_signal(Vec::<MetricDefinitionView>::new);
    let mut loading = use_signal(|| true);
    let mut error_msg = use_signal(|| Option::<String>::None);
    let mut reload_tick = use_signal(|| 0u32);

    let mut selected_id = use_signal(String::new);
    let mut selected_detail = use_signal(|| Option::<MetricDefinitionView>::None);
    let mut versions = use_signal(Vec::<MetricVersionView>::new);
    let mut detail_error = use_signal(|| Option::<String>::None);
    let mut detail_loading = use_signal(|| false);
    let mut action_banner = use_signal(|| Option::<String>::None);
    let mut mutating = use_signal(|| false);

    // Create form state.
    let mut show_create = use_signal(|| false);
    let mut new_key = use_signal(String::new);
    let mut new_display = use_signal(String::new);
    let mut new_polarity = use_signal(|| "higher_is_better".to_string());
    let mut new_metric_type = use_signal(|| "base".to_string());
    let mut new_formula = use_signal(String::new);
    let mut new_summary = use_signal(String::new);
    let mut new_lineage = use_signal(Vec::<LineageRef>::new);
    let mut create_error = use_signal(|| Option::<String>::None);
    let mut creating = use_signal(|| false);

    let can_author = caller_can_author(&auth.read());
    let is_admin = caller_is_admin(&auth.read());

    // ── Load metric list ────────────────────────────────────────
    use_effect(move || {
        let _tick = reload_tick.read();
        let token = match auth.read().token.clone() {
            Some(t) => t,
            None => return,
        };
        loading.set(true);
        error_msg.set(None);
        spawn(async move {
            match metrics::list(&token).await {
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
                    error_msg.set(Some(format!("Failed to load metrics: {}", err.message)));
                }
            }
        });
    });

    // ── Load selected metric details + versions ────────────────
    use_effect(move || {
        let id = selected_id.read().clone();
        if id.is_empty() {
            selected_detail.set(None);
            versions.set(Vec::new());
            return;
        }
        let _tick = reload_tick.read();
        let token = match auth.read().token.clone() {
            Some(t) => t,
            None => return,
        };
        detail_loading.set(true);
        detail_error.set(None);
        spawn(async move {
            let detail_res = metrics::get(&token, &id).await;
            let versions_res = metrics::list_versions(&token, &id).await;
            detail_loading.set(false);
            match detail_res {
                Ok(d) => selected_detail.set(Some(d)),
                Err(err) => {
                    if err.is_unauthorized() {
                        auth.set(AuthState::default());
                        AuthState::clear_storage();
                        navigator.push(AppRoute::Login {});
                        return;
                    }
                    detail_error.set(Some(format!("Failed to load metric: {}", err.message)));
                }
            }
            match versions_res {
                Ok(vs) => versions.set(vs),
                Err(err) => {
                    if !err.is_forbidden() {
                        detail_error
                            .set(Some(format!("Failed to load versions: {}", err.message)));
                    }
                }
            }
        });
    });

    // ── Create handler ──────────────────────────────────────────
    let do_create = move |evt: FormEvent| {
        evt.prevent_default();
        if creating() {
            return;
        }
        let key = new_key().trim().to_string();
        let display = new_display().trim().to_string();
        let formula = new_formula();
        if key.is_empty() || display.is_empty() || formula.trim().is_empty() {
            create_error.set(Some(
                "Key, display name, and formula are all required.".to_string(),
            ));
            return;
        }
        let token = match auth.read().token.clone() {
            Some(t) => t,
            None => return,
        };
        let summary_val = {
            let s = new_summary();
            if s.trim().is_empty() { None } else { Some(s) }
        };
        let input = MetricCreateInput {
            key_name: key,
            display_name: display,
            unit: None,
            polarity: new_polarity(),
            formula,
            description: None,
            metric_type: new_metric_type(),
            window_seconds: None,
            change_summary: summary_val,
            lineage_refs: new_lineage.read().clone(),
        };
        creating.set(true);
        create_error.set(None);
        spawn(async move {
            match metrics::create(&token, &input).await {
                Ok(_) => {
                    creating.set(false);
                    new_key.set(String::new());
                    new_display.set(String::new());
                    new_formula.set(String::new());
                    new_summary.set(String::new());
                    new_lineage.set(Vec::new());
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

    // ── Approve / publish actions ──────────────────────────────
    let do_approve = move |version_id: String| {
        if mutating() {
            return;
        }
        let Some(det) = selected_detail.read().clone() else {
            return;
        };
        let token = match auth.read().token.clone() {
            Some(t) => t,
            None => return,
        };
        mutating.set(true);
        action_banner.set(None);
        spawn(async move {
            match metrics::approve(&token, &det.id, &version_id).await {
                Ok(v) => {
                    mutating.set(false);
                    action_banner.set(Some(format!(
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
                    action_banner.set(Some(err.message));
                }
            }
        });
    };

    let do_publish = move |version_id: String| {
        if mutating() {
            return;
        }
        let Some(det) = selected_detail.read().clone() else {
            return;
        };
        let token = match auth.read().token.clone() {
            Some(t) => t,
            None => return,
        };
        mutating.set(true);
        action_banner.set(None);
        spawn(async move {
            match metrics::publish(&token, &det.id, &version_id).await {
                Ok(_) => {
                    mutating.set(false);
                    action_banner.set(Some("Version published.".to_string()));
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
                    action_banner.set(Some(err.message));
                }
            }
        });
    };

    let items_snapshot = items.read().clone();
    let detail_snapshot = selected_detail.read().clone();
    let versions_snapshot = versions.read().clone();
    let lineage_snapshot = new_lineage.read().clone();

    rsx! {
        div { class: "page-header library-header",
            div {
                h1 { "Metrics" }
                p { class: "text-muted",
                    "Catalog of metric definitions used across dashboards."
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
                    if show_create() { "Cancel" } else { "New metric" }
                }
            }
        }

        if show_create() && can_author {
            form { class: "create-form", onsubmit: do_create,
                h2 { "New metric definition" }
                if let Some(msg) = create_error() {
                    div { class: "error-banner", "{msg}" }
                }
                label { r#for: "new-metric-key", "Key name" }
                input {
                    id: "new-metric-key",
                    r#type: "text",
                    value: "{new_key()}",
                    oninput: move |evt| new_key.set(evt.value()),
                    required: true,
                }
                label { r#for: "new-metric-display", "Display name" }
                input {
                    id: "new-metric-display",
                    r#type: "text",
                    value: "{new_display()}",
                    oninput: move |evt| new_display.set(evt.value()),
                    required: true,
                }
                label { r#for: "new-metric-polarity", "Polarity" }
                select {
                    id: "new-metric-polarity",
                    value: "{new_polarity()}",
                    onchange: move |evt| new_polarity.set(evt.value()),
                    option { value: "higher_is_better", "Higher is better" }
                    option { value: "lower_is_better", "Lower is better" }
                    option { value: "neutral", "Neutral" }
                }
                label { r#for: "new-metric-type", "Metric type" }
                select {
                    id: "new-metric-type",
                    value: "{new_metric_type()}",
                    onchange: move |evt| new_metric_type.set(evt.value()),
                    option { value: "base", "Base" }
                    option { value: "derived", "Derived" }
                }
                label { r#for: "new-metric-formula", "Formula" }
                textarea {
                    id: "new-metric-formula",
                    rows: "4",
                    value: "{new_formula()}",
                    oninput: move |evt| new_formula.set(evt.value()),
                    required: true,
                }
                label { "Lineage refs" }
                div { class: "lineage-picker",
                    p { class: "text-muted",
                        "Click a metric below to add it as a lineage reference (uses its effective version)."
                    }
                    div { class: "lineage-chips",
                        for chip in lineage_snapshot.iter().cloned() {
                            span { class: "lineage-chip",
                                "{chip.definition_id}"
                                button {
                                    r#type: "button",
                                    class: "link-button",
                                    onclick: move |_| {
                                        let def_id = chip.definition_id.clone();
                                        new_lineage.with_mut(|v| {
                                            v.retain(|r| r.definition_id != def_id);
                                        });
                                    },
                                    "×"
                                }
                            }
                        }
                    }
                    div { class: "lineage-options",
                        for candidate in items_snapshot.iter().cloned() {
                            {
                                let def_id = candidate.id.clone();
                                let version_id = candidate
                                    .effective_version
                                    .as_ref()
                                    .map(|v| v.id.clone())
                                    .unwrap_or_default();
                                let disabled = version_id.is_empty();
                                let key_display = candidate.key_name.clone();
                                rsx! {
                                    button {
                                        r#type: "button",
                                        class: "link-button",
                                        disabled: disabled,
                                        onclick: move |_| {
                                            let already = new_lineage
                                                .read()
                                                .iter()
                                                .any(|r| r.definition_id == def_id);
                                            if !already && !version_id.is_empty() {
                                                new_lineage.with_mut(|v| {
                                                    v.push(LineageRef {
                                                        definition_id: def_id.clone(),
                                                        version_id: version_id.clone(),
                                                    });
                                                });
                                            }
                                        },
                                        "+ {key_display}"
                                    }
                                }
                            }
                        }
                    }
                }
                label { r#for: "new-metric-summary", "Change summary (optional)" }
                input {
                    id: "new-metric-summary",
                    r#type: "text",
                    value: "{new_summary()}",
                    oninput: move |evt| new_summary.set(evt.value()),
                }
                button {
                    r#type: "submit",
                    class: "primary-button",
                    disabled: creating(),
                    if creating() { "Saving..." } else { "Create metric" }
                }
            }
        }

        div { class: "metric-layout",
            // ── Left: metric list ─────────────────────────────
            section { class: "metric-list",
                h2 { "Definitions" }
                if loading() {
                    p { class: "text-muted", "Loading metrics..." }
                } else if let Some(msg) = error_msg() {
                    div { class: "error-banner", "{msg}" }
                } else if items_snapshot.is_empty() {
                    p { class: "text-muted", "No metrics defined yet." }
                } else {
                    ul { class: "metric-list__items",
                        for m in items_snapshot.iter().cloned() {
                            {
                                let row_id = m.id.clone();
                                let row_id_click = row_id.clone();
                                let display = m.display_name.clone();
                                let key_name = m.key_name.clone();
                                let state = m
                                    .effective_version
                                    .as_ref()
                                    .map(|v| v.state.clone())
                                    .unwrap_or_else(|| "-".to_string());
                                let state_for_class = state.clone();
                                let is_selected = selected_id.read().clone() == row_id;
                                let li_class = if is_selected {
                                    "metric-list__row selected"
                                } else {
                                    "metric-list__row"
                                };
                                rsx! {
                                    li { key: "{row_id}", class: "{li_class}",
                                        button {
                                            r#type: "button",
                                            class: "metric-list__button",
                                            onclick: move |_| selected_id.set(row_id_click.clone()),
                                            span { class: "metric-list__display", "{display}" }
                                            span { class: "metric-list__key text-muted", "{key_name}" }
                                            span { class: "metric-state-badge {state_for_class}", "{state}" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // ── Right: detail view ────────────────────────────
            section { class: "metric-detail",
                if let Some(msg) = action_banner() {
                    div { class: "save-banner", "{msg}" }
                }
                if let Some(msg) = detail_error() {
                    div { class: "error-banner", "{msg}" }
                }
                if detail_loading() {
                    p { class: "text-muted", "Loading metric..." }
                } else if let Some(det) = detail_snapshot.clone() {
                    MetricDetailView {
                        metric: det,
                        versions: versions_snapshot,
                        is_admin: is_admin,
                        can_author: can_author,
                        mutating: mutating(),
                        on_approve: do_approve,
                        on_publish: do_publish,
                    }
                } else {
                    p { class: "text-muted", "Select a metric on the left to view its details." }
                }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct MetricDetailProps {
    metric: MetricDefinitionView,
    versions: Vec<MetricVersionView>,
    is_admin: bool,
    can_author: bool,
    mutating: bool,
    on_approve: EventHandler<String>,
    on_publish: EventHandler<String>,
}

#[component]
fn MetricDetailView(props: MetricDetailProps) -> Element {
    let m = props.metric.clone();
    let effective = m.effective_version.clone();

    let effective_state = effective
        .as_ref()
        .map(|v| v.state.clone())
        .unwrap_or_else(|| "-".to_string());
    let state_for_badge = effective_state.clone();
    let effective_version_number = effective.as_ref().map(|v| v.version_number).unwrap_or(0);
    let formula = effective
        .as_ref()
        .map(|v| v.formula.clone())
        .unwrap_or_default();
    let metric_type = effective
        .as_ref()
        .map(|v| v.metric_type.clone())
        .unwrap_or_default();
    let window_seconds = effective
        .as_ref()
        .and_then(|v| v.window_seconds)
        .map(|n| n.to_string())
        .unwrap_or_else(|| "-".to_string());
    let description = effective
        .as_ref()
        .and_then(|v| v.description.clone())
        .unwrap_or_default();
    let lineage = effective
        .as_ref()
        .map(|v| v.lineage_refs.clone())
        .unwrap_or_default();

    let warn_draft = effective_state == "draft";

    let can_approve = props.can_author && effective_state == "draft";
    let can_publish = props.is_admin && effective_state == "approved";

    let effective_version_id = effective.as_ref().map(|v| v.id.clone()).unwrap_or_default();
    let approve_id = effective_version_id.clone();
    let publish_id = effective_version_id;

    rsx! {
        header { class: "metric-detail__header",
            div {
                h2 { "{m.display_name}" }
                p { class: "text-muted", "{m.key_name} · {m.polarity}" }
            }
            span { class: "metric-state-badge {state_for_badge}",
                "v{effective_version_number} {effective_state}"
            }
        }

        if warn_draft {
            div { class: "save-banner",
                "This metric's effective version is still a draft. Dashboards bound to it may need re-verification once it's published."
            }
        }

        dl { class: "course-meta",
            dt { "Metric type" }
            dd { "{metric_type}" }
            dt { "Window (seconds)" }
            dd { "{window_seconds}" }
            dt { "Unit" }
            dd {
                if let Some(u) = m.unit.as_ref() { "{u}" } else { "-" }
            }
        }

        h3 { "Formula" }
        if formula.is_empty() {
            p { class: "text-muted", "No formula defined yet." }
        } else {
            pre { class: "journal-body__content", "{formula}" }
        }

        if !description.is_empty() {
            h3 { "Description" }
            p { "{description}" }
        }

        if !lineage.is_empty() {
            h3 { "Lineage" }
            div { class: "lineage-chips",
                for r in lineage.iter().cloned() {
                    span { class: "lineage-chip", "{r.definition_id} @ {r.version_id}" }
                }
            }
        }

        div { class: "library-header__actions",
            if can_approve {
                button {
                    r#type: "button",
                    class: "primary-button",
                    disabled: props.mutating,
                    onclick: move |_| props.on_approve.call(approve_id.clone()),
                    "Approve v{effective_version_number}"
                }
            }
            if can_publish {
                button {
                    r#type: "button",
                    class: "publish-button",
                    disabled: props.mutating,
                    onclick: move |_| props.on_publish.call(publish_id.clone()),
                    "Publish v{effective_version_number}"
                }
            }
        }

        if !props.versions.is_empty() {
            section { class: "version-history",
                h2 { "Version history" }
                ul { class: "version-list",
                    for v in props.versions.iter().cloned() {
                        {
                            let version_id = v.id.clone();
                            let version_number = v.version_number;
                            let state = v.state.clone();
                            let state_class = state.clone();
                            let created_at = v.created_at.clone();
                            rsx! {
                                li { key: "{version_id}", class: "version-row",
                                    div { class: "version-row__meta",
                                        span { class: "metric-state-badge {state_class}",
                                            "v{version_number} {state}"
                                        }
                                        span { class: "text-muted", "{created_at}" }
                                    }
                                    if let Some(summary) = v.change_summary.as_ref() {
                                        p { class: "version-row__summary", "{summary}" }
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
