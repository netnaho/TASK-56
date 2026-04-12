//! Typed wrappers around the backend authentication endpoints.
//!
//! The backend emits roles as snake-case strings (`"admin"`,
//! `"librarian"`, ...) inside the user payload. Rather than write a
//! custom `Deserialize` impl for [`crate::types::User`] — which would
//! couple the domain type to the wire format — we deserialize into a
//! neutral [`RawUser`] first and then convert into [`User`] using
//! [`Role::from_str`]. Unknown role strings are silently dropped.

use serde::{Deserialize, Serialize};

use crate::api::client::{ApiClient, ApiError};
use crate::types::{Role, User};

/// Request body sent to `POST /auth/login`.
#[derive(Debug, Clone, Serialize)]
pub struct LoginInput {
    pub email: String,
    pub password: String,
}

/// Raw user payload as emitted by the backend (roles are strings).
#[derive(Debug, Clone, Deserialize)]
struct RawUser {
    id: String,
    email: String,
    display_name: String,
    #[serde(default)]
    roles: Vec<String>,
    #[serde(default)]
    department_id: Option<String>,
}

/// Raw login response shape matching the backend wire format.
#[derive(Debug, Clone, Deserialize)]
struct RawLoginResponse {
    token: String,
    expires_at: String,
    user: RawUser,
}

/// Typed login response exposed to the rest of the frontend.
#[derive(Debug, Clone)]
pub struct LoginResponse {
    pub token: String,
    pub expires_at: String,
    pub user: User,
}

/// Converts a [`RawUser`] into the domain [`User`] type by parsing the
/// snake-case role strings into [`Role`] variants. Unknown roles are
/// dropped.
fn raw_user_to_user(raw: RawUser) -> User {
    let roles = raw
        .roles
        .into_iter()
        .filter_map(|s| Role::from_str(&s))
        .collect();
    User {
        id: raw.id,
        email: raw.email,
        display_name: raw.display_name,
        roles,
        department_id: raw.department_id,
    }
}

/// Calls `POST /auth/login`. On success returns the triple
/// `(token, expires_at, user)`.
pub async fn login(
    email: String,
    password: String,
) -> Result<(String, String, User), ApiError> {
    let client = ApiClient::new(None);
    let body = LoginInput { email, password };
    let raw: RawLoginResponse = client.post_json("/auth/login", &body).await?;
    let user = raw_user_to_user(raw.user);
    Ok((raw.token, raw.expires_at, user))
}

/// Calls `POST /auth/logout` with a bearer token. The server returns no
/// body on success.
pub async fn logout(token: &str) -> Result<(), ApiError> {
    let client = ApiClient::new(Some(token.to_string()));
    client.post_no_body("/auth/logout").await
}

/// Calls `GET /auth/me` with a bearer token and returns the current
/// user record.
pub async fn me(token: &str) -> Result<User, ApiError> {
    let client = ApiClient::new(Some(token.to_string()));
    let raw: RawUser = client.get_json("/auth/me").await?;
    Ok(raw_user_to_user(raw))
}
