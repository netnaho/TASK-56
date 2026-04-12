//! Typed wrappers for the audit log endpoints.
//!
//! Audit log read requires the `AuditRead` capability (Admin only in the
//! default RBAC matrix). Chain verification also requires Admin.

use serde::{Deserialize, Serialize};

use crate::api::client::{ApiClient, ApiError};

/// A single audit log entry as returned by the backend.
#[derive(Debug, Clone, Deserialize)]
pub struct AuditLogEntry {
    pub id: String,
    pub sequence_number: i64,
    pub actor_id: Option<String>,
    pub actor_email: Option<String>,
    pub action: String,
    pub target_entity_type: Option<String>,
    pub target_entity_id: Option<String>,
    pub ip_address: Option<String>,
    pub created_at: String,
    pub current_hash: String,
}

/// Envelope returned by GET /api/v1/audit-logs.
#[derive(Debug, Clone, Deserialize)]
pub struct AuditLogEnvelope {
    pub entries: Vec<AuditLogEntry>,
    pub count: usize,
}

/// Chain integrity status.
#[derive(Debug, Clone, Deserialize)]
pub struct ChainStatus {
    pub total_entries: i64,
    pub valid: bool,
    pub broken_at_sequence: Option<i64>,
    pub message: String,
}

/// Query parameters for listing audit logs.
#[derive(Debug, Default, Serialize)]
pub struct AuditLogQuery {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actor_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_entity_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to: Option<String>,
    pub limit: u32,
}

fn client(token: &str) -> ApiClient {
    ApiClient::new(Some(token.to_string()))
}

/// GET /api/v1/audit-logs — fetch entries with optional filters.
pub async fn list(token: &str, q: &AuditLogQuery) -> Result<AuditLogEnvelope, ApiError> {
    let mut params = vec![format!("limit={}", q.limit)];
    if let Some(ref v) = q.action {
        params.push(format!("action={}", v));
    }
    if let Some(ref v) = q.actor_id {
        params.push(format!("actor_id={}", v));
    }
    if let Some(ref v) = q.target_entity_type {
        params.push(format!("target_entity_type={}", v));
    }
    if let Some(ref v) = q.from {
        params.push(format!("from={}", v));
    }
    if let Some(ref v) = q.to {
        params.push(format!("to={}", v));
    }
    let path = format!("/audit-logs?{}", params.join("&"));
    client(token).get_json(&path).await
}

/// GET /api/v1/audit-logs/verify-chain — admin-only chain integrity check.
pub async fn verify_chain(token: &str) -> Result<ChainStatus, ApiError> {
    client(token).get_json("/audit-logs/verify-chain").await
}
