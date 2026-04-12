//! Application state definitions for the Scholarly frontend.
//!
//! These structs are intended to be provided as Dioxus context values
//! at the app root so that any component can access shared state.

use js_sys::Date;
use serde::{Deserialize, Serialize};
use wasm_bindgen::JsValue;

use crate::types::{Role, User};

/// Key used to persist [`AuthState`] in browser `localStorage`.
const STORAGE_KEY: &str = "scholarly_auth";

/// Authentication state — tracks the currently logged-in user and their
/// bearer token.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct AuthState {
    /// The authenticated user, if any.
    pub user: Option<User>,
    /// JWT or opaque bearer token for API requests.
    pub token: Option<String>,
    /// RFC3339 timestamp at which the current token expires.
    pub expires_at: Option<String>,
}

impl AuthState {
    /// Returns `true` if a user is currently authenticated.
    ///
    /// Requires both a bearer token and a hydrated user record.  Also
    /// validates the stored `expires_at` timestamp so that a stale session
    /// in localStorage redirects to login immediately rather than waiting
    /// for the first failing API call.
    pub fn is_authenticated(&self) -> bool {
        if self.token.is_none() || self.user.is_none() {
            return false;
        }
        // If we have an expiry timestamp, reject tokens that are already past
        // their expiry time according to the browser clock.  A NaN result from
        // `get_time()` (invalid date string) is treated as "not expired" —
        // the backend session validation will catch the edge case.
        if let Some(ref exp) = self.expires_at {
            let expires_ms = Date::new(&JsValue::from_str(exp)).get_time();
            let now_ms = Date::now();
            if !expires_ms.is_nan() && now_ms >= expires_ms {
                return false;
            }
        }
        true
    }

    /// Returns the user's highest-privileged role, if any.
    pub fn primary_role(&self) -> Option<Role> {
        let user = self.user.as_ref()?;
        user.roles
            .iter()
            .max_by_key(|r| r.level())
            .cloned()
    }

    /// Attempts to hydrate an [`AuthState`] from `localStorage`.
    ///
    /// Returns [`AuthState::default`] on any failure (missing window,
    /// unavailable storage, missing key, malformed JSON).
    pub fn load_from_storage() -> AuthState {
        let storage = match web_sys::window().and_then(|w| w.local_storage().ok().flatten()) {
            Some(s) => s,
            None => return AuthState::default(),
        };

        let raw = match storage.get_item(STORAGE_KEY) {
            Ok(Some(s)) => s,
            _ => return AuthState::default(),
        };

        serde_json::from_str::<AuthState>(&raw).unwrap_or_default()
    }

    /// Persists this [`AuthState`] into `localStorage`. All errors are
    /// swallowed — persistence is best-effort.
    pub fn save_to_storage(&self) {
        let Some(storage) = web_sys::window().and_then(|w| w.local_storage().ok().flatten()) else {
            return;
        };
        if let Ok(json) = serde_json::to_string(self) {
            let _ = storage.set_item(STORAGE_KEY, &json);
        }
    }

    /// Removes the persisted auth state from `localStorage`.
    pub fn clear_storage() {
        if let Some(storage) = web_sys::window().and_then(|w| w.local_storage().ok().flatten()) {
            let _ = storage.remove_item(STORAGE_KEY);
        }
    }
}

/// Global application state — holds non-auth concerns shared across the
/// app (e.g., theme preference, feature flags).
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct AppState {
    /// Whether the sidebar is collapsed.
    pub sidebar_collapsed: bool,
}
