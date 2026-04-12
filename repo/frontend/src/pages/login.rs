//! Login page — user authentication.
//!
//! This page is rendered *without* the main layout sidebar so that
//! unauthenticated users see a clean, focused login experience.

use dioxus::prelude::*;
use dioxus_router::prelude::*;

use crate::api::auth;
use crate::router::AppRoute;
use crate::state::AuthState;

/// Returns `true` if `email` looks vaguely like an address (a single
/// `@` with text on both sides and a dot in the domain). This is a
/// client-side convenience check only — the backend is always the
/// source of truth.
fn is_valid_email(email: &str) -> bool {
    let trimmed = email.trim();
    let Some(at) = trimmed.find('@') else {
        return false;
    };
    let (local, domain) = trimmed.split_at(at);
    let domain = &domain[1..];
    !local.is_empty() && !domain.is_empty() && domain.contains('.')
}

/// Renders the login form and handles credential submission.
#[component]
pub fn Login() -> Element {
    let mut auth_state = use_context::<Signal<AuthState>>();
    let navigator = use_navigator();

    // Redirect already-authenticated users straight to the dashboard.
    // This handles the case where the browser reloads on /login but
    // localStorage still holds a valid session.
    use_effect(move || {
        if auth_state.read().is_authenticated() {
            navigator.push(AppRoute::Dashboard {});
        }
    });

    let mut email = use_signal(String::new);
    let mut password = use_signal(String::new);
    let mut error = use_signal(|| Option::<String>::None);
    let mut submitting = use_signal(|| false);

    let mut do_login = move || {
        if submitting() {
            return;
        }

        let email_val = email().trim().to_string();
        let password_val = password();

        if !is_valid_email(&email_val) {
            error.set(Some("Please enter a valid email address.".to_string()));
            return;
        }
        if password_val.len() < 12 {
            error.set(Some(
                "Password must be at least 12 characters long.".to_string(),
            ));
            return;
        }

        error.set(None);
        submitting.set(true);

        spawn(async move {
            match auth::login(email_val, password_val).await {
                Ok((token, expires_at, user)) => {
                    auth_state.with_mut(|s| {
                        s.token = Some(token);
                        s.expires_at = Some(expires_at);
                        s.user = Some(user);
                    });
                    auth_state.read().save_to_storage();
                    submitting.set(false);
                    navigator.push(AppRoute::Dashboard {});
                }
                Err(err) => {
                    submitting.set(false);
                    error.set(Some(err.message));
                }
            }
        });
    };

    rsx! {
        div { class: "login-page",
            div { class: "login-card",
                h1 { class: "login-card__brand", "Scholarly" }
                p { class: "login-card__subtitle",
                    "Sign in to access the library resources system."
                }

                if let Some(msg) = error() {
                    div { class: "login-error", "{msg}" }
                }

                div { class: "login-form",

                    label { r#for: "email", "Email" }
                    input {
                        id: "email",
                        r#type: "email",
                        name: "email",
                        autocomplete: "username",
                        value: "{email}",
                        oninput: move |evt| email.set(evt.value()),
                        onkeydown: move |evt| {
                            if evt.key() == Key::Enter { do_login(); }
                        },
                    }

                    label { r#for: "password", "Password" }
                    input {
                        id: "password",
                        r#type: "password",
                        name: "password",
                        autocomplete: "current-password",
                        value: "{password}",
                        oninput: move |evt| password.set(evt.value()),
                        onkeydown: move |evt| {
                            if evt.key() == Key::Enter { do_login(); }
                        },
                    }

                    button {
                        r#type: "button",
                        disabled: submitting(),
                        onclick: move |_| do_login(),
                        if submitting() { "Signing in..." } else { "Sign in" }
                    }
                }

                p { class: "login-card__hint",
                    "Default seed passwords are documented in the README."
                }
            }
        }
    }
}
