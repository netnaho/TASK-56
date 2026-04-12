//! Authentication hook — provides access to the current auth state.

use dioxus::prelude::*;

use crate::state::AuthState;

/// Returns a clone of the current [`AuthState`] from context.
///
/// Panics if called outside a component tree that provides
/// `Signal<AuthState>` (typically mounted at the app root).
pub fn use_auth() -> AuthState {
    use_context::<Signal<AuthState>>().read().clone()
}
