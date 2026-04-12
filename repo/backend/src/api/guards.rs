//! Rocket request guards that back the authorization layer.
//!
//! Four guards are exported:
//!
//! * [`RequestId`] — populated unconditionally for every request so error
//!   envelopes always carry a correlation id. Not an auth gate on its own.
//! * [`ClientContext`] — extracts the client IP address and User-Agent
//!   into an owned struct so handlers can forward them to the audit log
//!   without taking a raw `&Request<'_>`.
//! * [`AuthedPrincipal`] — succeeds only when the request carries a valid
//!   `Authorization: Bearer <token>` whose opaque session is still active.
//! * [`AdminOnly`] — super-set of `AuthedPrincipal` that additionally
//!   requires `Role::Admin`.
//!
//! The pool and config are pulled from Rocket's managed state inside the
//! guard so handlers don't have to re-pass them.

use rocket::http::Status;
use rocket::outcome::Outcome;
use rocket::request::{self, FromRequest, Request};
use rocket::State;
use sqlx::MySqlPool;
use uuid::Uuid;

use crate::application::auth_service;
use crate::application::principal::{Principal, Role};
use crate::application::session;
use crate::errors::AppError;

// ---------------------------------------------------------------------------
// RequestId
// ---------------------------------------------------------------------------

/// Per-request correlation identifier. Put into `Request::local_cache` so
/// both the error responder and log lines can read it.
#[derive(Clone, Debug)]
pub struct RequestId(pub String);

#[rocket::async_trait]
impl<'r> FromRequest<'r> for RequestId {
    type Error = std::convert::Infallible;
    async fn from_request(req: &'r Request<'_>) -> request::Outcome<Self, Self::Error> {
        let id = req
            .local_cache(|| RequestId(Uuid::new_v4().to_string()))
            .clone();
        Outcome::Success(id)
    }
}

// ---------------------------------------------------------------------------
// ClientContext — IP address + User-Agent, owned
// ---------------------------------------------------------------------------

/// Captures the ambient HTTP client context that service code needs for
/// audit logging and lockout tracking. Infallible — always succeeds.
#[derive(Debug, Clone, Default)]
pub struct ClientContext {
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
}

#[rocket::async_trait]
impl<'r> FromRequest<'r> for ClientContext {
    type Error = std::convert::Infallible;
    async fn from_request(req: &'r Request<'_>) -> request::Outcome<Self, Self::Error> {
        // Prime the request id so error envelopes correlate even on the
        // unauthenticated path.
        let _ = req.local_cache(|| RequestId(Uuid::new_v4().to_string()));
        Outcome::Success(ClientContext {
            ip_address: request_ip(req),
            user_agent: request_user_agent(req),
        })
    }
}

// ---------------------------------------------------------------------------
// AuthedPrincipal
// ---------------------------------------------------------------------------

/// Authenticated principal guard.
///
/// Rejects with 401 if the Authorization header is missing or malformed,
/// or if the token is unknown/expired/revoked.
#[derive(Debug, Clone)]
pub struct AuthedPrincipal(pub Principal);

impl AuthedPrincipal {
    pub fn into_inner(self) -> Principal {
        self.0
    }
    pub fn as_ref(&self) -> &Principal {
        &self.0
    }
}

fn extract_bearer(req: &Request<'_>) -> Option<String> {
    let header = req.headers().get_one("Authorization")?;
    let trimmed = header.trim();
    if trimmed.len() > 7 && trimmed[..7].eq_ignore_ascii_case("Bearer ") {
        Some(trimmed[7..].trim().to_string())
    } else {
        None
    }
}

#[rocket::async_trait]
impl<'r> FromRequest<'r> for AuthedPrincipal {
    type Error = AppError;

    async fn from_request(req: &'r Request<'_>) -> request::Outcome<Self, Self::Error> {
        let _ = req.local_cache(|| RequestId(Uuid::new_v4().to_string()));

        let Some(token) = extract_bearer(req) else {
            return Outcome::Error((Status::Unauthorized, AppError::Unauthorized));
        };

        let pool = match req.guard::<&State<MySqlPool>>().await {
            Outcome::Success(p) => p,
            _ => {
                return Outcome::Error((
                    Status::InternalServerError,
                    AppError::Internal("database pool not managed".into()),
                ))
            }
        };

        let session_record = match session::find_active_by_token(pool.inner(), &token).await {
            Ok(s) => s,
            Err(e) => {
                let status = e.status();
                return Outcome::Error((status, e));
            }
        };

        match auth_service::load_principal_for_session(pool.inner(), &session_record).await {
            Ok(principal) => Outcome::Success(AuthedPrincipal(principal)),
            Err(e) => {
                let status = e.status();
                Outcome::Error((status, e))
            }
        }
    }
}

// ---------------------------------------------------------------------------
// AdminOnly
// ---------------------------------------------------------------------------

/// Admin-only guard. Fails with 403 if the principal is not `Role::Admin`.
#[derive(Debug, Clone)]
pub struct AdminOnly(pub Principal);

impl AdminOnly {
    pub fn into_inner(self) -> Principal {
        self.0
    }
    pub fn as_ref(&self) -> &Principal {
        &self.0
    }
}

#[rocket::async_trait]
impl<'r> FromRequest<'r> for AdminOnly {
    type Error = AppError;

    async fn from_request(req: &'r Request<'_>) -> request::Outcome<Self, Self::Error> {
        match req.guard::<AuthedPrincipal>().await {
            Outcome::Success(inner) => {
                if inner.0.has_role(Role::Admin) {
                    Outcome::Success(AdminOnly(inner.0))
                } else {
                    Outcome::Error((Status::Forbidden, AppError::Forbidden))
                }
            }
            Outcome::Error(e) => Outcome::Error(e),
            Outcome::Forward(f) => Outcome::Forward(f),
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

pub fn request_ip(req: &Request<'_>) -> Option<String> {
    if let Some(xff) = req.headers().get_one("X-Forwarded-For") {
        if let Some(first) = xff.split(',').next() {
            return Some(first.trim().to_string());
        }
    }
    req.client_ip().map(|ip| ip.to_string())
}

pub fn request_user_agent(req: &Request<'_>) -> Option<String> {
    req.headers().get_one("User-Agent").map(|s| s.to_string())
}
