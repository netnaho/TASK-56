//! API client hook — provides a configured HTTP client for backend calls.

use dioxus::prelude::*;

use crate::api::client::ApiClient;
use crate::state::AuthState;

/// Returns a configured [`ApiClient`] instance seeded with the token
/// from the current [`AuthState`] context, so every outbound request
/// is authenticated automatically.
pub fn use_api() -> ApiClient {
    let token = use_context::<Signal<AuthState>>().read().token.clone();
    ApiClient::new(token)
}
