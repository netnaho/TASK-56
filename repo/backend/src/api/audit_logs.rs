//! Audit log routes: search, CSV export, and hash-chain verification.
//!
//! Three routes are mounted here, each with a distinct authorization gate:
//!
//! | Route                          | Guard / Capability     | Roles        |
//! |--------------------------------|------------------------|--------------|
//! | `GET /`  (list)                | `Capability::AuditRead`   | Admin, Auditor |
//! | `GET /export.csv`              | `Capability::AuditExport` | Admin only     |
//! | `GET /verify-chain`            | `AdminOnly` guard         | Admin only     |
//!
//! Every successful access appends a self-referential audit entry:
//! `audit.search`, `audit.export`, or `audit.chain.verify` respectively.

use chrono::{DateTime, Utc};
use rocket::serde::json::Json;
use rocket::State;
use serde::Serialize;
use sqlx::MySqlPool;
use uuid::Uuid;

use crate::api::download::BinaryDownload;
use crate::api::guards::{AdminOnly, AuthedPrincipal};
use crate::application::audit_service::{
    self, actions, AuditEvent, AuditLogView, AuditSearch, ChainStatus,
};
use crate::application::authorization::{self, Capability};
use crate::application::masking;
use crate::errors::{AppError, AppResult};

pub fn routes() -> Vec<rocket::Route> {
    routes![list_audit_logs, verify_chain_endpoint, export_csv]
}

#[derive(Serialize)]
pub struct AuditLogEnvelope {
    pub entries: Vec<AuditLogView>,
    pub count: usize,
}

/// GET /api/v1/audit-logs?actor_id=&action=&from=&to=&limit=
#[get("/?<actor_id>&<action>&<target_entity_type>&<target_entity_id>&<from>&<to>&<limit>")]
pub async fn list_audit_logs(
    principal: AuthedPrincipal,
    pool: &State<MySqlPool>,
    actor_id: Option<String>,
    action: Option<String>,
    target_entity_type: Option<String>,
    target_entity_id: Option<String>,
    from: Option<String>,
    to: Option<String>,
    limit: Option<u32>,
) -> AppResult<Json<AuditLogEnvelope>> {
    let principal = principal.into_inner();
    authorization::require(&principal, Capability::AuditRead)?;

    let filter = AuditSearch {
        actor_id: parse_uuid_opt(actor_id.as_deref(), "actor_id")?,
        action,
        target_entity_type,
        target_entity_id: parse_uuid_opt(target_entity_id.as_deref(), "target_entity_id")?,
        from: parse_rfc3339_opt(from.as_deref(), "from")?,
        to: parse_rfc3339_opt(to.as_deref(), "to")?,
        limit: limit.unwrap_or(100),
    };

    let mut entries = audit_service::search(pool.inner(), &filter).await?;

    // Apply masking for non-admin viewers. Admin = no redaction.
    for row in entries.iter_mut() {
        if let Some(ref email) = row.actor_email {
            row.actor_email = Some(masking::mask_email_for_audit(email, &principal));
        }
        row.ip_address = masking::mask_ip_for_audit(row.ip_address.as_deref(), &principal);
    }

    // Record the search itself to the audit log.
    let _ = audit_service::record(
        pool.inner(),
        AuditEvent {
            actor_id: Some(principal.user_id),
            actor_email: Some(&principal.email),
            action: actions::AUDIT_SEARCH,
            target_entity_type: Some("audit_log"),
            target_entity_id: None,
            change_payload: Some(serde_json::json!({
                "count": entries.len(),
                "filter": {
                    "actor_id": filter.actor_id,
                    "action": filter.action,
                    "target_entity_type": filter.target_entity_type,
                }
            })),
            ip_address: None,
            user_agent: None,
        },
    )
    .await;

    let count = entries.len();
    Ok(Json(AuditLogEnvelope { entries, count }))
}

/// GET /api/v1/audit-logs/export.csv?actor_id=&action=&from=&to=&limit=
///
/// Export audit log entries as a UTF-8 CSV file.
///
/// Requires `Capability::AuditExport` (Admin-only per the current capability
/// matrix). Masking rules are applied identically to the JSON listing endpoint
/// so the same email-hash and IP-redaction policies hold. The export is itself
/// recorded in the audit log as an `audit.export` event.
///
/// Response headers:
/// - `Content-Type: text/csv; charset=utf-8`
/// - `Content-Disposition: attachment; filename="audit_logs_<YYYYMMDD_HHMMSS>.csv"`
///
/// Default limit for export is 500 (server-capped maximum). Pass `limit=` to
/// reduce the window. Supports all the same filter parameters as
/// `GET /api/v1/audit-logs`.
#[get("/export.csv?<actor_id>&<action>&<target_entity_type>&<target_entity_id>&<from>&<to>&<limit>")]
pub async fn export_csv(
    principal: AuthedPrincipal,
    pool: &State<MySqlPool>,
    actor_id: Option<String>,
    action: Option<String>,
    target_entity_type: Option<String>,
    target_entity_id: Option<String>,
    from: Option<String>,
    to: Option<String>,
    limit: Option<u32>,
) -> AppResult<BinaryDownload> {
    let principal = principal.into_inner();
    authorization::require(&principal, Capability::AuditExport)?;

    let filter = AuditSearch {
        actor_id: parse_uuid_opt(actor_id.as_deref(), "actor_id")?,
        action,
        target_entity_type,
        target_entity_id: parse_uuid_opt(target_entity_id.as_deref(), "target_entity_id")?,
        from: parse_rfc3339_opt(from.as_deref(), "from")?,
        to: parse_rfc3339_opt(to.as_deref(), "to")?,
        // Default to the server-capped maximum for exports; callers may lower it.
        limit: limit.unwrap_or(500),
    };

    let mut entries = audit_service::search(pool.inner(), &filter).await?;

    // Apply the same masking rules as the JSON listing endpoint so that
    // non-admin callers (if AuditExport is ever extended to Auditor) still
    // see hashed emails and redacted IPs.
    for row in entries.iter_mut() {
        if let Some(ref email) = row.actor_email {
            row.actor_email = Some(masking::mask_email_for_audit(email, &principal));
        }
        row.ip_address = masking::mask_ip_for_audit(row.ip_address.as_deref(), &principal);
    }

    // Record the export to the audit log (best-effort; do not fail the export).
    let _ = audit_service::record(
        pool.inner(),
        AuditEvent {
            actor_id: Some(principal.user_id),
            actor_email: Some(&principal.email),
            action: actions::AUDIT_EXPORT,
            target_entity_type: Some("audit_log"),
            target_entity_id: None,
            change_payload: Some(serde_json::json!({
                "row_count": entries.len(),
                "filter": {
                    "actor_id": filter.actor_id,
                    "action": filter.action,
                    "target_entity_type": filter.target_entity_type,
                    "from": filter.from,
                    "to": filter.to,
                }
            })),
            ip_address: None,
            user_agent: None,
        },
    )
    .await;

    let bytes = audit_service::export_to_csv(&entries);
    let filename = format!(
        "audit_logs_{}.csv",
        chrono::Utc::now().format("%Y%m%d_%H%M%S"),
    );

    Ok(BinaryDownload {
        bytes,
        mime: "text/csv; charset=utf-8",
        filename,
    })
}

/// GET /api/v1/audit-logs/verify-chain — admin-only chain integrity check.
#[get("/verify-chain")]
pub async fn verify_chain_endpoint(
    admin: AdminOnly,
    pool: &State<MySqlPool>,
) -> AppResult<Json<ChainStatus>> {
    let admin = admin.into_inner();
    let status = audit_service::verify_chain(pool.inner()).await?;

    let _ = audit_service::record(
        pool.inner(),
        AuditEvent {
            actor_id: Some(admin.user_id),
            actor_email: Some(&admin.email),
            action: actions::AUDIT_CHAIN_VERIFY,
            target_entity_type: Some("audit_chain"),
            target_entity_id: None,
            change_payload: Some(serde_json::json!({
                "total_entries": status.total_entries,
                "valid": status.valid,
                "broken_at_sequence": status.broken_at_sequence,
            })),
            ip_address: None,
            user_agent: None,
        },
    )
    .await;

    Ok(Json(status))
}

fn parse_uuid_opt(raw: Option<&str>, field: &str) -> AppResult<Option<Uuid>> {
    match raw {
        None => Ok(None),
        Some(s) if s.is_empty() => Ok(None),
        Some(s) => Uuid::parse_str(s)
            .map(Some)
            .map_err(|_| AppError::Validation(format!("{} must be a UUID", field))),
    }
}

fn parse_rfc3339_opt(raw: Option<&str>, field: &str) -> AppResult<Option<DateTime<Utc>>> {
    match raw {
        None => Ok(None),
        Some(s) if s.is_empty() => Ok(None),
        Some(s) => DateTime::parse_from_rfc3339(s)
            .map(|d| Some(d.with_timezone(&Utc)))
            .map_err(|_| AppError::Validation(format!("{} must be RFC3339", field))),
    }
}
