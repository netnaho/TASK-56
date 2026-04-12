//! Forbidden (403) page — shown when a user attempts to access a
//! resource their role does not allow.

use dioxus::prelude::*;
use dioxus_router::prelude::*;

use crate::router::AppRoute;

/// Renders a simple 403 message and a link back to the dashboard.
#[component]
pub fn ForbiddenPage() -> Element {
    rsx! {
        div { class: "page-header",
            h1 { "Access denied" }
            p { "You do not have access to this page." }
            p {
                Link { to: AppRoute::Dashboard {}, "Return to dashboard" }
            }
        }
    }
}
