//! Immutable audit log with tamper-evident hash chain.
//!
//! # Append path
//!
//! 1. Application services call [`record`] (or one of the `record_*`
//!    convenience wrappers) inside the same database transaction that
//!    performed the change (or immediately after, for read events).
//! 2. [`record`] serializes the event, computes
//!    `current_hash = SHA-256(previous_hash || sequence || payload_bytes)`,
//!    and writes two rows: one into `audit_logs` and one into
//!    `audit_hash_chain`.
//! 3. There is **no update path**. `audit_logs` and `audit_hash_chain` are
//!    strictly INSERT-only at the application layer; any attempted UPDATE
//!    goes through no service method here.
//!
//! # Verification path
//!
//! [`verify_chain`] walks the chain in sequence order and recomputes every
//! hash. It returns a [`ChainStatus`] describing the first inconsistency,
//! if any. The `/api/v1/audit-logs/verify-chain` admin route exposes this.
//!
//! # Masking
//!
//! Audit rows returned via the API pass through
//! [`crate::application::masking`] helpers so non-admin viewers see masked
//! actor emails and IPs.

use chrono::{DateTime, NaiveDateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{MySqlPool, Row};
use uuid::Uuid;

use crate::errors::{AppError, AppResult};

/// Canonical action strings. Use constants, not free-form strings, so the
/// set of audit actions is greppable and static.
pub mod actions {
    pub const LOGIN_SUCCESS: &str = "auth.login.success";
    pub const LOGIN_FAILURE: &str = "auth.login.failure";
    pub const LOGIN_LOCKED: &str = "auth.login.locked";
    pub const LOGOUT: &str = "auth.logout";
    pub const PASSWORD_CHANGE: &str = "auth.password.change";
    pub const PASSWORD_BOOTSTRAP: &str = "auth.password.bootstrap";
    pub const PERMISSION_GRANT: &str = "rbac.permission.grant";
    pub const PERMISSION_REVOKE: &str = "rbac.permission.revoke";
    pub const ADMIN_CONFIG_WRITE: &str = "admin.config.write";
    pub const ADMIN_CONFIG_READ: &str = "admin.config.read";
    pub const AUDIT_CHAIN_VERIFY: &str = "audit.chain.verify";
    pub const AUDIT_SEARCH: &str = "audit.search";
    pub const AUDIT_EXPORT: &str = "audit.export";

    // Phase 6 — reports
    pub const REPORT_CREATE: &str = "report.create";
    pub const REPORT_UPDATE: &str = "report.update";
    pub const REPORT_RUN_TRIGGER: &str = "report.run.trigger";
    pub const REPORT_RUN_COMPLETE: &str = "report.run.complete";
    pub const REPORT_RUN_FAIL: &str = "report.run.fail";
    pub const REPORT_ARTIFACT_DOWNLOAD: &str = "report.artifact.download";
    pub const REPORT_SCHEDULE_CREATE: &str = "report.schedule.create";
    pub const REPORT_SCHEDULE_UPDATE: &str = "report.schedule.update";
    pub const REPORT_SCHEDULE_DELETE: &str = "report.schedule.delete";

    // Phase 6 — exports
    pub const EXPORT_REPORT_GENERATE: &str = "export.report.generate";

    // Phase 6 — retention
    pub const RETENTION_POLICY_CREATE: &str = "retention.policy.create";
    pub const RETENTION_POLICY_UPDATE: &str = "retention.policy.update";
    pub const RETENTION_EXECUTE: &str = "retention.execute";
    pub const RETENTION_EXECUTE_POLICY: &str = "retention.execute.policy";
    /// Emitted when strict-mode retention is blocked because actionable
    /// legacy artifacts (artifact_dek IS NULL) remain in the expiry window.
    /// Payload: `unresolved_count`, `policy_id`.  Never contains file paths.
    pub const RETENTION_STRICT_MODE_BLOCKED: &str = "retention.strict_mode_blocked";

    // Phase 6 — encryption / config
    pub const ENCRYPTION_CONFIG_CHANGE: &str = "encryption.config.change";

    // Phase 6 hardened — artifact backfill
    /// Emitted when an admin triggers the legacy-artifact backfill process.
    pub const ARTIFACT_BACKFILL_START: &str = "artifact.backfill.start";
    /// Emitted once per batch with aggregated counts (never contains DEK or path material).
    pub const ARTIFACT_BACKFILL_BATCH: &str = "artifact.backfill.batch";
    /// Emitted on successful completion of the full backfill pass.
    pub const ARTIFACT_BACKFILL_COMPLETE: &str = "artifact.backfill.complete";
    /// Emitted when a single artifact fails to backfill (sanitized error only).
    pub const ARTIFACT_BACKFILL_ROW_FAILURE: &str = "artifact.backfill.row_failure";

    // Phase 7 — user management
    pub const USER_CREATE: &str = "user.create";
    pub const USER_UPDATE: &str = "user.update";
    pub const USER_DEACTIVATE: &str = "user.deactivate";
}

/// Single event written by [`record`].
#[derive(Debug, Clone, Serialize)]
pub struct AuditEvent<'a> {
    pub actor_id: Option<Uuid>,
    pub actor_email: Option<&'a str>,
    pub action: &'a str,
    pub target_entity_type: Option<&'a str>,
    pub target_entity_id: Option<Uuid>,
    pub change_payload: Option<serde_json::Value>,
    pub ip_address: Option<&'a str>,
    pub user_agent: Option<&'a str>,
}

/// Row returned by audit search, with masking already applied by the caller.
#[derive(Debug, Clone, Serialize)]
pub struct AuditLogView {
    pub id: Uuid,
    pub sequence_number: i64,
    pub actor_id: Option<Uuid>,
    pub actor_email: Option<String>,
    pub action: String,
    pub target_entity_type: Option<String>,
    pub target_entity_id: Option<Uuid>,
    pub ip_address: Option<String>,
    pub created_at: NaiveDateTime,
    pub current_hash: String,
}

/// Search filter used by the `/audit-logs` listing endpoint.
#[derive(Debug, Clone, Deserialize)]
pub struct AuditSearch {
    pub actor_id: Option<Uuid>,
    pub action: Option<String>,
    pub target_entity_type: Option<String>,
    pub target_entity_id: Option<Uuid>,
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
    #[serde(default = "default_limit")]
    pub limit: u32,
}

impl Default for AuditSearch {
    fn default() -> Self {
        Self {
            actor_id: None,
            action: None,
            target_entity_type: None,
            target_entity_id: None,
            from: None,
            to: None,
            limit: default_limit(),
        }
    }
}

fn default_limit() -> u32 {
    100
}

/// Result of walking the chain.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ChainStatus {
    pub total_entries: i64,
    pub valid: bool,
    pub broken_at_sequence: Option<i64>,
    pub message: String,
}

/// Append a new audit event. Writes two rows atomically via a transaction.
pub async fn record(pool: &MySqlPool, event: AuditEvent<'_>) -> AppResult<Uuid> {
    let mut tx = pool
        .begin()
        .await
        .map_err(|e| AppError::Database(format!("audit tx begin: {}", e)))?;

    let audit_id = Uuid::new_v4();
    let payload_json = event.change_payload.as_ref().map(|v| {
        serde_json::to_string(v).unwrap_or_else(|e| {
            tracing::warn!(
                action = event.action,
                "failed to serialize audit change_payload, storing null: {}",
                e
            );
            "null".into()
        })
    });

    sqlx::query(
        r#"
        INSERT INTO audit_logs
            (id, actor_id, actor_email, action,
             target_entity_type, target_entity_id,
             change_payload, ip_address, user_agent)
        VALUES (?, ?, ?, ?, ?, ?, CAST(? AS JSON), ?, ?)
        "#,
    )
    .bind(audit_id.to_string())
    .bind(event.actor_id.map(|u| u.to_string()))
    .bind(event.actor_email)
    .bind(event.action)
    .bind(event.target_entity_type)
    .bind(event.target_entity_id.map(|u| u.to_string()))
    .bind(payload_json.as_deref().unwrap_or("null"))
    .bind(event.ip_address)
    .bind(event.user_agent)
    .execute(&mut *tx)
    .await
    .map_err(|e| AppError::Database(format!("audit_logs insert: {}", e)))?;

    // Obtain the previous chain tip under the same transaction so concurrent
    // writers can't collide on sequence_number.
    let tip_row = sqlx::query(
        r#"
        SELECT sequence_number, current_hash
          FROM audit_hash_chain
         ORDER BY sequence_number DESC
         LIMIT 1 FOR UPDATE
        "#,
    )
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| AppError::Database(format!("audit tip read: {}", e)))?;

    let (prev_seq, prev_hash): (i64, Option<String>) = match tip_row {
        None => (0, None),
        Some(row) => {
            let seq: i64 = row
                .try_get("sequence_number")
                .map_err(|e| AppError::Database(e.to_string()))?;
            let hash: String = row
                .try_get("current_hash")
                .map_err(|e| AppError::Database(e.to_string()))?;
            (seq, Some(hash))
        }
    };
    let sequence = prev_seq + 1;

    let current_hash = compute_hash(
        prev_hash.as_deref(),
        sequence,
        audit_id,
        event.action,
        payload_json.as_deref(),
    );

    sqlx::query(
        r#"
        INSERT INTO audit_hash_chain
            (id, audit_log_id, sequence_number, previous_hash, current_hash)
        VALUES (?, ?, ?, ?, ?)
        "#,
    )
    .bind(Uuid::new_v4().to_string())
    .bind(audit_id.to_string())
    .bind(sequence)
    .bind(prev_hash)
    .bind(&current_hash)
    .execute(&mut *tx)
    .await
    .map_err(|e| AppError::Database(format!("audit_hash_chain insert: {}", e)))?;

    tx.commit()
        .await
        .map_err(|e| AppError::Database(format!("audit tx commit: {}", e)))?;

    Ok(audit_id)
}

/// Compute the SHA-256 hex digest for a chain entry.
///
/// The input layout is `previous_hash || 0x7c || sequence || 0x7c ||
/// audit_id || 0x7c || action || 0x7c || payload`. `0x7c` is `|`, used as
/// an unambiguous field separator. Exposed so unit tests can reproduce
/// the exact bytes.
pub fn compute_hash(
    previous_hash: Option<&str>,
    sequence_number: i64,
    audit_log_id: Uuid,
    action: &str,
    payload_json: Option<&str>,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(previous_hash.unwrap_or("").as_bytes());
    hasher.update(b"|");
    hasher.update(sequence_number.to_be_bytes());
    hasher.update(b"|");
    hasher.update(audit_log_id.as_bytes());
    hasher.update(b"|");
    hasher.update(action.as_bytes());
    hasher.update(b"|");
    hasher.update(payload_json.unwrap_or("null").as_bytes());
    hex::encode(hasher.finalize())
}

/// Walk the entire chain in sequence order and verify every link.
pub async fn verify_chain(pool: &MySqlPool) -> AppResult<ChainStatus> {
    let rows = sqlx::query(
        r#"
        SELECT c.sequence_number, c.previous_hash, c.current_hash,
               c.audit_log_id, l.action,
               CAST(l.change_payload AS CHAR) AS payload_text
          FROM audit_hash_chain c
          JOIN audit_logs       l ON l.id = c.audit_log_id
         ORDER BY c.sequence_number ASC
        "#,
    )
    .fetch_all(pool)
    .await
    .map_err(|e| AppError::Database(format!("verify_chain: {}", e)))?;

    let mut expected_prev: Option<String> = None;
    let mut expected_seq: i64 = 1;
    let total = rows.len() as i64;

    for row in rows {
        let seq: i64 = row
            .try_get("sequence_number")
            .map_err(|e| AppError::Database(e.to_string()))?;
        let prev: Option<String> = row
            .try_get("previous_hash")
            .map_err(|e| AppError::Database(e.to_string()))?;
        let curr: String = row
            .try_get("current_hash")
            .map_err(|e| AppError::Database(e.to_string()))?;
        let audit_id_str: String = row
            .try_get("audit_log_id")
            .map_err(|e| AppError::Database(e.to_string()))?;
        let action: String = row
            .try_get("action")
            .map_err(|e| AppError::Database(e.to_string()))?;
        let payload: Option<String> = row
            .try_get("payload_text")
            .map_err(|e| AppError::Database(e.to_string()))?;

        if seq != expected_seq {
            return Ok(ChainStatus {
                total_entries: total,
                valid: false,
                broken_at_sequence: Some(seq),
                message: format!("sequence gap: expected {} got {}", expected_seq, seq),
            });
        }
        if prev != expected_prev {
            return Ok(ChainStatus {
                total_entries: total,
                valid: false,
                broken_at_sequence: Some(seq),
                message: format!("previous_hash mismatch at sequence {}", seq),
            });
        }
        let audit_uuid = Uuid::parse_str(&audit_id_str)
            .map_err(|e| AppError::Database(e.to_string()))?;
        // MySQL's CAST(JSON AS CHAR) adds spaces after ':' and ',' separators,
        // but the hash was computed at write time from serde_json's compact output
        // (no spaces). Normalise to compact JSON before recomputing so the two
        // representations agree.
        let normalized_payload: Option<String> = payload.as_deref().map(|p| {
            serde_json::from_str::<serde_json::Value>(p)
                .ok()
                .and_then(|v| serde_json::to_string(&v).ok())
                .unwrap_or_else(|| p.to_string())
        });
        let recomputed = compute_hash(
            expected_prev.as_deref(),
            seq,
            audit_uuid,
            &action,
            normalized_payload.as_deref(),
        );
        if recomputed != curr {
            return Ok(ChainStatus {
                total_entries: total,
                valid: false,
                broken_at_sequence: Some(seq),
                message: format!("current_hash mismatch at sequence {}", seq),
            });
        }
        expected_prev = Some(curr);
        expected_seq += 1;
    }

    Ok(ChainStatus {
        total_entries: total,
        valid: true,
        broken_at_sequence: None,
        message: "chain verified".into(),
    })
}

/// Paginated audit search. Callers must have already passed
/// `Capability::AuditRead`. Masking is applied by the caller via
/// [`crate::application::masking`] before the rows go over the wire.
pub async fn search(pool: &MySqlPool, filter: &AuditSearch) -> AppResult<Vec<AuditLogView>> {
    // Cap the limit server-side no matter what the client asked for.
    let limit = filter.limit.clamp(1, 500) as i64;

    let mut sql = String::from(
        r#"
        SELECT l.id, c.sequence_number, l.actor_id, l.actor_email, l.action,
               l.target_entity_type, l.target_entity_id, l.ip_address,
               l.created_at, c.current_hash
          FROM audit_logs l
          JOIN audit_hash_chain c ON c.audit_log_id = l.id
         WHERE 1=1
        "#,
    );
    if filter.actor_id.is_some() {
        sql.push_str(" AND l.actor_id = ?");
    }
    if filter.action.is_some() {
        sql.push_str(" AND l.action = ?");
    }
    if filter.target_entity_type.is_some() {
        sql.push_str(" AND l.target_entity_type = ?");
    }
    if filter.target_entity_id.is_some() {
        sql.push_str(" AND l.target_entity_id = ?");
    }
    if filter.from.is_some() {
        sql.push_str(" AND l.created_at >= ?");
    }
    if filter.to.is_some() {
        sql.push_str(" AND l.created_at <= ?");
    }
    sql.push_str(" ORDER BY c.sequence_number DESC LIMIT ?");

    let mut q = sqlx::query(&sql);
    if let Some(id) = filter.actor_id {
        q = q.bind(id.to_string());
    }
    if let Some(ref a) = filter.action {
        q = q.bind(a.clone());
    }
    if let Some(ref t) = filter.target_entity_type {
        q = q.bind(t.clone());
    }
    if let Some(id) = filter.target_entity_id {
        q = q.bind(id.to_string());
    }
    if let Some(from) = filter.from {
        q = q.bind(from.naive_utc());
    }
    if let Some(to) = filter.to {
        q = q.bind(to.naive_utc());
    }
    q = q.bind(limit);

    let rows = q
        .fetch_all(pool)
        .await
        .map_err(|e| AppError::Database(format!("audit search: {}", e)))?;

    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let id: String = row
            .try_get("id")
            .map_err(|e| AppError::Database(e.to_string()))?;
        let seq: i64 = row
            .try_get("sequence_number")
            .map_err(|e| AppError::Database(e.to_string()))?;
        let actor_id: Option<String> = row
            .try_get("actor_id")
            .map_err(|e| AppError::Database(e.to_string()))?;
        let actor_email: Option<String> = row
            .try_get("actor_email")
            .map_err(|e| AppError::Database(e.to_string()))?;
        let action: String = row
            .try_get("action")
            .map_err(|e| AppError::Database(e.to_string()))?;
        let target_entity_type: Option<String> = row
            .try_get("target_entity_type")
            .map_err(|e| AppError::Database(e.to_string()))?;
        let target_entity_id: Option<String> = row
            .try_get("target_entity_id")
            .map_err(|e| AppError::Database(e.to_string()))?;
        let ip_address: Option<String> = row
            .try_get("ip_address")
            .map_err(|e| AppError::Database(e.to_string()))?;
        let created_at: NaiveDateTime = row
            .try_get("created_at")
            .map_err(|e| AppError::Database(e.to_string()))?;
        let current_hash: String = row
            .try_get("current_hash")
            .map_err(|e| AppError::Database(e.to_string()))?;

        out.push(AuditLogView {
            id: Uuid::parse_str(&id).map_err(|e| AppError::Database(e.to_string()))?,
            sequence_number: seq,
            actor_id: actor_id.as_deref().and_then(|s| Uuid::parse_str(s).ok()),
            actor_email,
            action,
            target_entity_type,
            target_entity_id: target_entity_id
                .as_deref()
                .and_then(|s| Uuid::parse_str(s).ok()),
            ip_address,
            created_at,
            current_hash,
        });
    }

    Ok(out)
}

/// Serialize a slice of (already-masked) [`AuditLogView`] entries to CSV bytes.
///
/// The caller is responsible for applying masking rules before passing entries
/// here, just as the JSON listing endpoint does. Column order (fixed):
///
/// `sequence_number, id, actor_id, actor_email, action, target_entity_type,
///  target_entity_id, ip_address, created_at, current_hash`
pub fn export_to_csv(entries: &[AuditLogView]) -> Vec<u8> {
    let mut out = String::new();
    out.push_str(
        "sequence_number,id,actor_id,actor_email,action,\
         target_entity_type,target_entity_id,ip_address,created_at,current_hash\n",
    );
    for e in entries {
        out.push_str(&csv_cell(&e.sequence_number.to_string()));
        out.push(',');
        out.push_str(&csv_cell(&e.id.to_string()));
        out.push(',');
        out.push_str(&csv_cell(
            &e.actor_id.map(|u| u.to_string()).unwrap_or_default(),
        ));
        out.push(',');
        out.push_str(&csv_cell(e.actor_email.as_deref().unwrap_or("")));
        out.push(',');
        out.push_str(&csv_cell(&e.action));
        out.push(',');
        out.push_str(&csv_cell(
            e.target_entity_type.as_deref().unwrap_or(""),
        ));
        out.push(',');
        out.push_str(&csv_cell(
            &e.target_entity_id
                .map(|u| u.to_string())
                .unwrap_or_default(),
        ));
        out.push(',');
        out.push_str(&csv_cell(e.ip_address.as_deref().unwrap_or("")));
        out.push(',');
        out.push_str(&csv_cell(&e.created_at.to_string()));
        out.push(',');
        out.push_str(&csv_cell(&e.current_hash));
        out.push('\n');
    }
    out.into_bytes()
}

/// Quote a CSV cell value when it contains commas, double-quotes, or newlines.
/// Inner double-quotes are escaped by doubling them (RFC 4180).
fn csv_cell(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') || s.contains('\r') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compute_hash_is_deterministic() {
        let a = compute_hash(None, 1, Uuid::nil(), "auth.login.success", Some("null"));
        let b = compute_hash(None, 1, Uuid::nil(), "auth.login.success", Some("null"));
        assert_eq!(a, b);
    }

    #[test]
    fn hash_changes_when_any_input_changes() {
        let base = compute_hash(None, 1, Uuid::nil(), "x", Some("null"));
        assert_ne!(base, compute_hash(Some("prev"), 1, Uuid::nil(), "x", Some("null")));
        assert_ne!(base, compute_hash(None, 2, Uuid::nil(), "x", Some("null")));
        assert_ne!(base, compute_hash(None, 1, Uuid::new_v4(), "x", Some("null")));
        assert_ne!(base, compute_hash(None, 1, Uuid::nil(), "y", Some("null")));
        assert_ne!(base, compute_hash(None, 1, Uuid::nil(), "x", Some("{\"a\":1}")));
    }

    #[test]
    fn chain_detects_tamper() {
        // Simulate a 3-entry chain in memory.
        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();
        let id3 = Uuid::new_v4();
        let h1 = compute_hash(None, 1, id1, "a", Some("null"));
        let h2 = compute_hash(Some(&h1), 2, id2, "b", Some("null"));
        let h3 = compute_hash(Some(&h2), 3, id3, "c", Some("null"));

        // Tamper with entry 2 and recompute forward — h3 must differ,
        // proving the chain detects modification.
        let h2_bad = compute_hash(Some(&h1), 2, id2, "b_TAMPERED", Some("null"));
        let h3_following = compute_hash(Some(&h2_bad), 3, id3, "c", Some("null"));
        assert_ne!(h3, h3_following);
    }

    #[test]
    fn payload_affects_hash() {
        let id = Uuid::new_v4();
        let a = compute_hash(None, 1, id, "x", Some("{\"v\":1}"));
        let b = compute_hash(None, 1, id, "x", Some("{\"v\":2}"));
        assert_ne!(a, b);
    }

    #[test]
    fn default_search_limit_is_capped() {
        let s = AuditSearch::default();
        assert_eq!(s.limit, 100);
    }

    // ── export_to_csv ─────────────────────────────────────────────────────────

    fn sample_entry(seq: i64, action: &str, email: Option<&str>) -> AuditLogView {
        AuditLogView {
            id: Uuid::nil(),
            sequence_number: seq,
            actor_id: None,
            actor_email: email.map(|s| s.to_string()),
            action: action.to_string(),
            target_entity_type: None,
            target_entity_id: None,
            ip_address: None,
            created_at: chrono::NaiveDateTime::from_timestamp_opt(0, 0).unwrap(),
            current_hash: "abc123".to_string(),
        }
    }

    #[test]
    fn export_to_csv_empty_returns_header_only() {
        let csv = export_to_csv(&[]);
        let text = String::from_utf8(csv).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 1, "empty export should have exactly one header line");
        assert!(lines[0].starts_with("sequence_number,id,"));
        assert!(lines[0].ends_with(",current_hash"));
    }

    #[test]
    fn export_to_csv_one_row_has_correct_column_count() {
        let entry = sample_entry(1, "auth.login.success", Some("admin@scholarly.local"));
        let csv = export_to_csv(&[entry]);
        let text = String::from_utf8(csv).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 2, "one entry → header + one data row");
        // 10 columns → 9 commas (unquoted row)
        let data_commas = lines[1].chars().filter(|&c| c == ',').count();
        assert_eq!(data_commas, 9, "data row must have 9 commas for 10 columns");
    }

    #[test]
    fn export_to_csv_escapes_commas_in_fields() {
        let mut entry = sample_entry(1, "auth.login.success", None);
        entry.action = "action,with,commas".to_string();
        let csv = export_to_csv(&[entry]);
        let text = String::from_utf8(csv).unwrap();
        assert!(
            text.contains("\"action,with,commas\""),
            "field containing comma must be quoted: {text}"
        );
    }

    #[test]
    fn export_to_csv_escapes_embedded_quotes() {
        let mut entry = sample_entry(1, "x", None);
        entry.action = "say \"hello\"".to_string();
        let csv = export_to_csv(&[entry]);
        let text = String::from_utf8(csv).unwrap();
        assert!(
            text.contains("\"say \"\"hello\"\"\""),
            "embedded double-quotes must be doubled per RFC 4180: {text}"
        );
    }

    #[test]
    fn export_to_csv_multiple_rows_correct_count() {
        let entries: Vec<AuditLogView> = (1..=5)
            .map(|i| sample_entry(i, "auth.login.success", None))
            .collect();
        let csv = export_to_csv(&entries);
        let text = String::from_utf8(csv).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 6, "header + 5 data rows");
    }
}
