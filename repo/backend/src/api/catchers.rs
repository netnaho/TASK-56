//! Custom Rocket error catchers.
//!
//! These catchers intercept HTTP error responses that fall through the normal
//! handler chain (unmatched routes, guard failures that Rocket handles
//! directly, etc.) and format them in the same error envelope that every
//! `AppError` produces:
//!
//! ```json
//! { "error": { "code": "...", "message": "...", "request_id": "...", "fields": [] } }
//! ```
//!
//! Without custom catchers, Rocket emits its own JSON format for unmatched
//! routes (404) and body-deserialization failures (422), which breaks any
//! client that expects the documented envelope.

use rocket::serde::json::Json;
use rocket::Request;
use serde_json::Value;
use uuid::Uuid;

pub fn catchers() -> Vec<rocket::Catcher> {
    catchers![
        bad_request,
        unauthorized,
        forbidden,
        not_found,
        unprocessable_entity,
        internal_error
    ]
}

/// Produce a standard error envelope with a fresh request-correlation ID.
fn envelope(code: &str, message: &str) -> Json<Value> {
    Json(serde_json::json!({
        "error": {
            "code": code,
            "message": message,
            "request_id": Uuid::new_v4().to_string(),
            "fields": []
        }
    }))
}

#[catch(400)]
pub fn bad_request(_req: &Request) -> Json<Value> {
    envelope("bad_request", "The request could not be understood by the server.")
}

#[catch(401)]
pub fn unauthorized(_req: &Request) -> Json<Value> {
    envelope("unauthorized", "Authentication is required to access this resource.")
}

#[catch(403)]
pub fn forbidden(_req: &Request) -> Json<Value> {
    envelope("forbidden", "You do not have permission to perform this action.")
}

#[catch(404)]
pub fn not_found(_req: &Request) -> Json<Value> {
    envelope("not_found", "The requested resource was not found.")
}

#[catch(422)]
pub fn unprocessable_entity(_req: &Request) -> Json<Value> {
    envelope(
        "validation_error",
        "The request body could not be processed. Check the Content-Type and field formats.",
    )
}

#[catch(500)]
pub fn internal_error(_req: &Request) -> Json<Value> {
    envelope("internal_error", "An unexpected server error occurred.")
}
