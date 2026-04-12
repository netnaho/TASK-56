//! Health check endpoint — liveness and readiness probe.
//!
//! `GET /health` — no authentication required. Returns a JSON object
//! indicating whether the server process is alive and whether the database
//! connection pool can reach the database.
//!
//! Expected response shapes:
//!
//! ```json
//! { "status": "ok",       "database": "ok"    }
//! { "status": "degraded", "database": "error", "message": "database connectivity check failed" }
//! ```
//!
//! The HTTP status code is 200 in both cases so that load-balancer liveness
//! probes don't restart the container just because the DB is briefly
//! unavailable. Orchestration layers that care about full readiness should
//! inspect the `status` field.
//!
//! # Security note
//!
//! Raw database error strings are **never** included in the response body.
//! They may contain driver internals, connection strings, server hostnames,
//! or credentials and must not be forwarded to untrusted callers. The real
//! error is logged server-side at `WARN` level with full detail for ops
//! observability.

use rocket::serde::json::Json;
use rocket::State;
use serde::Serialize;
use sqlx::MySqlPool;

pub fn routes() -> Vec<rocket::Route> {
    routes![health_check]
}

/// The fixed client-visible message emitted when the DB check fails.
///
/// Deliberately vague: it confirms *that* the database is unreachable without
/// leaking *why* (driver errors, hostnames, credentials, query text, etc.).
const DEGRADED_DB_MESSAGE: &str = "database connectivity check failed";

/// JSON body returned by `GET /health`.
#[derive(Debug, Serialize, PartialEq)]
pub struct HealthResponse {
    pub status: &'static str,
    pub database: &'static str,
    /// Human-readable detail — present only on degraded responses.
    /// Always a static string; never raw driver error text.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<&'static str>,
}

/// Build the healthy variant. Extracted so tests can assert the shape
/// without a running database.
fn healthy() -> HealthResponse {
    HealthResponse {
        status: "ok",
        database: "ok",
        message: None,
    }
}

/// Build the degraded variant. The `db_error` argument is logged
/// server-side but never forwarded to the caller.
fn degraded(db_error: &sqlx::Error) -> HealthResponse {
    // Log full driver detail for ops while keeping the client response generic.
    tracing::warn!(
        error = %db_error,
        "health check: database connectivity check failed"
    );
    HealthResponse {
        status: "degraded",
        database: "error",
        message: Some(DEGRADED_DB_MESSAGE),
    }
}

/// GET /health — liveness/readiness probe.
///
/// Attempts a lightweight `SELECT 1` against the pool to verify DB
/// reachability. Does **not** require authentication.
#[get("/health")]
pub async fn health_check(pool: &State<MySqlPool>) -> Json<HealthResponse> {
    let response = match sqlx::query("SELECT 1").execute(pool.inner()).await {
        Ok(_) => healthy(),
        Err(ref e) => degraded(e),
    };
    Json(response)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // ── healthy() ─────────────────────────────────────────────────────────

    #[test]
    fn healthy_response_has_ok_status_and_database() {
        let r = healthy();
        assert_eq!(r.status, "ok");
        assert_eq!(r.database, "ok");
    }

    #[test]
    fn healthy_response_omits_message_field() {
        let r = healthy();
        assert!(
            r.message.is_none(),
            "healthy response must not include a message field"
        );
    }

    #[test]
    fn healthy_response_serialises_without_message_key() {
        let r = healthy();
        let json = serde_json::to_value(&r).expect("serialisation failed");
        assert_eq!(json["status"], "ok");
        assert_eq!(json["database"], "ok");
        assert!(
            json.get("message").is_none(),
            "message key must be absent in healthy JSON: {json}"
        );
    }

    // ── degraded() ────────────────────────────────────────────────────────

    /// Build a fake sqlx::Error that carries a recognisably internal message.
    /// `sqlx::Error::Configuration` wraps a `Box<dyn Error>` and its
    /// `Display` includes the inner message — perfect for asserting that
    /// the message is NOT forwarded to the client.
    fn fake_db_error_with_internals() -> sqlx::Error {
        sqlx::Error::Configuration(
            "Access denied for user 'scholarly_app'@'db-host' (using password: YES)".into(),
        )
    }

    #[test]
    fn degraded_response_has_degraded_status_and_error_database() {
        let e = fake_db_error_with_internals();
        let r = degraded(&e);
        assert_eq!(r.status, "degraded");
        assert_eq!(r.database, "error");
    }

    #[test]
    fn degraded_message_is_the_generic_constant_not_raw_error() {
        let e = fake_db_error_with_internals();
        let r = degraded(&e);
        let msg = r.message.expect("degraded response must include a message field");

        // Must be the static constant — no raw driver details.
        assert_eq!(msg, DEGRADED_DB_MESSAGE);
    }

    #[test]
    fn degraded_message_does_not_contain_sql_or_driver_internals() {
        let e = fake_db_error_with_internals();
        let r = degraded(&e);
        let msg = r.message.unwrap_or("");

        // The raw error string contains these substrings; the response must not.
        let forbidden = ["Access denied", "scholarly_app", "db-host", "password"];
        for fragment in forbidden {
            assert!(
                !msg.contains(fragment),
                "client-visible message must not contain '{fragment}': got '{msg}'"
            );
        }
    }

    #[test]
    fn degraded_response_serialises_with_message_key() {
        let e = fake_db_error_with_internals();
        let r = degraded(&e);
        let json = serde_json::to_value(&r).expect("serialisation failed");

        assert_eq!(json["status"], "degraded");
        assert_eq!(json["database"], "error");
        assert_eq!(json["message"], DEGRADED_DB_MESSAGE);
    }

    // ── schema stability ──────────────────────────────────────────────────

    /// Clients must be able to distinguish healthy from degraded by the
    /// `status` field alone (the documented contract). This test asserts
    /// the two variants are distinguishable and that `database` tracks
    /// `status` consistently.
    #[test]
    fn healthy_and_degraded_are_distinguishable_by_status() {
        let e = fake_db_error_with_internals();
        let h = healthy();
        let d = degraded(&e);

        assert_ne!(h.status, d.status);
        assert_ne!(h.database, d.database);
    }
}
