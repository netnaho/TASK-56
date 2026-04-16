//! Scholarly Frontend — Application Entry Point
//!
//! Initializes the Dioxus web application and launches the router.

mod api;
mod components;
mod hooks;
mod layouts;
mod pages;
mod router;
mod state;
mod types;

// Frontend unit tests — see src/tests/ for identifiable per-module test files.
#[cfg(test)]
mod tests;

use dioxus::prelude::*;
use dioxus_router::prelude::Router;

use router::AppRoute;
use state::AuthState;

/// Application root component.
///
/// Provides global state context and renders the router, which delegates
/// to individual page components via the declared route enum.
fn App() -> Element {
    use_context_provider(|| Signal::new(AuthState::load_from_storage()));

    rsx! {
        Router::<AppRoute> {}
    }
}

fn main() {
    dioxus::launch(App);
}
