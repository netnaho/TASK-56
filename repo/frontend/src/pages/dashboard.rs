//! Phase 5 dashboards home page.
//!
//! Renders seven independent `<DashboardPanelCard>` components —
//! course popularity, fill rate, drop rate, instructor workload, foot
//! traffic, dwell time, and interaction quality — each backed by its
//! own backend endpoint and its own loading / error state.

use chrono::{DateTime, NaiveDate, TimeZone, Utc};
use dioxus::prelude::*;
use dioxus_router::prelude::*;

use crate::api::dashboards;
use crate::components::dashboard_panel_card::{
    prettify_key, DashboardPanelCard, PanelState,
};
use crate::router::AppRoute;
use crate::state::AuthState;
use crate::types::Role;

fn caller_can_scope_department(auth: &AuthState) -> bool {
    auth.user
        .as_ref()
        .map(|u| {
            u.roles
                .iter()
                .any(|r| matches!(r, Role::Admin | Role::Librarian))
        })
        .unwrap_or(false)
}

/// Returns the default window: last 30 days ending at "now".
fn default_window() -> (DateTime<Utc>, DateTime<Utc>) {
    let now = Utc::now();
    let from = now - chrono::Duration::days(30);
    (from, now)
}

/// Parses a browser `<input type="date">` value (`YYYY-MM-DD`) into a
/// UTC midnight timestamp. Returns `None` on malformed input.
fn parse_date(s: &str) -> Option<DateTime<Utc>> {
    if s.is_empty() {
        return None;
    }
    let date = NaiveDate::parse_from_str(s, "%Y-%m-%d").ok()?;
    let naive = date.and_hms_opt(0, 0, 0)?;
    Utc.from_local_datetime(&naive).single()
}

/// Formats a UTC timestamp as `YYYY-MM-DD` for the date inputs.
fn format_date(ts: DateTime<Utc>) -> String {
    ts.format("%Y-%m-%d").to_string()
}

#[component]
pub fn Dashboard() -> Element {
    let mut auth = use_context::<Signal<AuthState>>();
    let navigator = use_navigator();

    let (default_from, default_to) = default_window();

    let mut from_input = use_signal(|| format_date(default_from));
    let mut to_input = use_signal(|| format_date(default_to));
    let mut department_input = use_signal(String::new);
    let mut reload_tick = use_signal(|| 0u32);

    let can_scope_department = caller_can_scope_department(&auth.read());

    let mut course_popularity = use_signal(|| PanelState::Loading);
    let mut fill_rate = use_signal(|| PanelState::Loading);
    let mut drop_rate = use_signal(|| PanelState::Loading);
    let mut instructor_workload = use_signal(|| PanelState::Loading);
    let mut foot_traffic = use_signal(|| PanelState::Loading);
    let mut dwell_time = use_signal(|| PanelState::Loading);
    let mut interaction_quality = use_signal(|| PanelState::Loading);

    // ── Load every panel whenever filters change ───────────────
    use_effect(move || {
        let _tick = reload_tick.read();
        let token = match auth.read().token.clone() {
            Some(t) => t,
            None => return,
        };
        let from_ts = parse_date(&from_input.read());
        let to_ts = parse_date(&to_input.read());
        let dept = {
            let s = department_input.read().clone();
            if s.trim().is_empty() { None } else { Some(s) }
        };

        course_popularity.set(PanelState::Loading);
        fill_rate.set(PanelState::Loading);
        drop_rate.set(PanelState::Loading);
        instructor_workload.set(PanelState::Loading);
        foot_traffic.set(PanelState::Loading);
        dwell_time.set(PanelState::Loading);
        interaction_quality.set(PanelState::Loading);

        spawn(async move {
            let dept_ref = dept.as_deref();

            let cp = dashboards::course_popularity(&token, from_ts, to_ts, dept_ref).await;
            if let Err(ref err) = cp {
                if err.is_unauthorized() {
                    auth.set(AuthState::default());
                    AuthState::clear_storage();
                    navigator.push(AppRoute::Login {});
                    return;
                }
            }
            course_popularity.set(PanelState::from_result(cp));

            fill_rate.set(PanelState::from_result(
                dashboards::fill_rate(&token, from_ts, to_ts, dept_ref).await,
            ));
            drop_rate.set(PanelState::from_result(
                dashboards::drop_rate(&token, from_ts, to_ts, dept_ref).await,
            ));
            instructor_workload.set(PanelState::from_result(
                dashboards::instructor_workload(&token, from_ts, to_ts, dept_ref).await,
            ));
            foot_traffic.set(PanelState::from_result(
                dashboards::foot_traffic(&token, from_ts, to_ts, dept_ref).await,
            ));
            dwell_time.set(PanelState::from_result(
                dashboards::dwell_time(&token, from_ts, to_ts, dept_ref).await,
            ));
            interaction_quality.set(PanelState::from_result(
                dashboards::interaction_quality(&token, from_ts, to_ts, dept_ref).await,
            ));
        });
    });

    let on_apply = move |evt: FormEvent| {
        evt.prevent_default();
        reload_tick.with_mut(|n| *n += 1);
    };

    rsx! {
        div { class: "page-header",
            h1 { "Dashboard" }
            p { class: "text-muted",
                "Approximate operational metrics for the selected window."
            }
        }

        form { class: "dashboard-filter", onsubmit: on_apply,
            label { r#for: "dashboard-from", "From" }
            input {
                id: "dashboard-from",
                r#type: "date",
                value: "{from_input()}",
                oninput: move |evt| from_input.set(evt.value()),
            }
            label { r#for: "dashboard-to", "To" }
            input {
                id: "dashboard-to",
                r#type: "date",
                value: "{to_input()}",
                oninput: move |evt| to_input.set(evt.value()),
            }
            if can_scope_department {
                label { r#for: "dashboard-dept", "Department ID (optional)" }
                input {
                    id: "dashboard-dept",
                    r#type: "text",
                    value: "{department_input()}",
                    oninput: move |evt| department_input.set(evt.value()),
                }
            }
            button {
                r#type: "submit",
                class: "primary-button",
                "Apply"
            }
        }

        div { class: "dashboard-grid",
            DashboardPanelCard {
                title: prettify_key("course_popularity"),
                state: course_popularity(),
            }
            DashboardPanelCard {
                title: prettify_key("fill_rate"),
                state: fill_rate(),
            }
            DashboardPanelCard {
                title: prettify_key("drop_rate"),
                state: drop_rate(),
            }
            DashboardPanelCard {
                title: prettify_key("instructor_workload"),
                state: instructor_workload(),
            }
            DashboardPanelCard {
                title: prettify_key("foot_traffic"),
                state: foot_traffic(),
            }
            DashboardPanelCard {
                title: prettify_key("dwell_time"),
                state: dwell_time(),
            }
            DashboardPanelCard {
                title: prettify_key("interaction_quality"),
                state: interaction_quality(),
            }
        }
    }
}
