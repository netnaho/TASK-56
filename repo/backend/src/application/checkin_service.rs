//! One-tap check-in with duplicate suppression and a single reasoned retry.
//!
//! # Behavioural contract
//!
//! * The **first** check-in for a given `(user_id, section_id)` inside
//!   the configured duplicate window succeeds and writes a row with
//!   `retry_sequence = 0`.
//! * A **second** attempt inside the window without calling the retry
//!   endpoint is rejected with `AppError::Conflict` and recorded with
//!   `is_duplicate_attempt = true` so the evidence is never lost.
//! * A retry must call the dedicated [`retry_checkin`] function with a
//!   `reason_code` from `checkin_retry_reasons`. Only **one** retry per
//!   original attempt is allowed (configurable via
//!   `checkin.max_retry_count`; default `1`).
//! * Every attempt — success, duplicate, or retry — captures the
//!   browser-available device fingerprint JSON, the client IP, the
//!   optional admin-configured local-network hint, and the result of
//!   the IP/CIDR "on-campus" rule.
//!
//! # Unified network-rule policy
//!
//! **Network verification failure is a hard rejection (HTTP 403) on BOTH
//! the initial and retry endpoints.**  The behaviour is symmetric:
//!
//! * A blocked attempt is persisted (`is_duplicate_attempt = true`) so
//!   the evidence is never lost, but the retry slot is **not consumed**.
//!   The caller may re-attempt once their device is on an allowed network.
//! * The audit log records `checkin.network_blocked` in both cases (with
//!   `"context": "retry_attempt"` added for retries).
//! * `CheckinStatus::NetworkBlocked` is kept in the domain enum for
//!   backwards-compatible JSON deserialisation but is no longer emitted
//!   by either endpoint.
//!
//! # Local-network enforcement — the truthful version
//!
//! Web browsers **cannot read the current Wi-Fi SSID** in a normal
//! security context. Phase 2 seeded an `network.approved_ssids` setting
//! for display and operator guidance, but Phase 5 enforces
//! "on-campus-ness" at the **server** via the client IP:
//! `admin_settings.checkin.allowed_client_cidrs` (a JSON array of CIDRs
//! like `"10.0.0.0/8"`). The check runs inside [`verify_network_rule`]
//! and records the outcome in `network_verified`. Empty list ⇒ the rule
//! is disabled; every attempt passes.
//!
//! The client-provided `network_hint` (e.g. the SSID the user claims to
//! be connected to) is recorded verbatim on the row so operators have an
//! honest paper trail — it is **not** trusted for enforcement.

use chrono::{Duration, NaiveDateTime, Utc};
use ipnet::IpNet;
use serde::{Deserialize, Serialize};
use sqlx::{MySqlPool, Row};
use std::net::IpAddr;
use std::str::FromStr;
use uuid::Uuid;

use super::audit_service::{self, AuditEvent};
use super::authorization::{principal_can, require, Capability};
use super::principal::{Principal, Role};
use crate::errors::{AppError, AppResult};

// ---------------------------------------------------------------------------
// Inputs and view models
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct CheckinInput {
    pub section_id: Uuid,
    pub checkin_type: CheckinType,
    /// Opaque JSON captured by the browser — user agent, viewport,
    /// platform, locale, timezone, etc. Stored verbatim; never trusted
    /// for authorization.
    #[serde(default)]
    pub device_fingerprint: Option<serde_json::Value>,
    /// Client-side-reported local network hint (e.g. the user-perceived
    /// SSID). Display-only; not used for authorization.
    #[serde(default)]
    pub network_hint: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CheckinRetryInput {
    pub reason_code: String,
    #[serde(default)]
    pub device_fingerprint: Option<serde_json::Value>,
    #[serde(default)]
    pub network_hint: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckinType {
    QrCode,
    Geofence,
    ManualInstructor,
    NfcBeacon,
}

impl CheckinType {
    pub fn as_db(self) -> &'static str {
        match self {
            CheckinType::QrCode => "qr_code",
            CheckinType::Geofence => "geofence",
            CheckinType::ManualInstructor => "manual_instructor",
            CheckinType::NfcBeacon => "nfc_beacon",
        }
    }
    pub fn from_db(s: &str) -> Option<Self> {
        match s {
            "qr_code" => Some(CheckinType::QrCode),
            "geofence" => Some(CheckinType::Geofence),
            "manual_instructor" => Some(CheckinType::ManualInstructor),
            "nfc_beacon" => Some(CheckinType::NfcBeacon),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct CheckinView {
    pub id: Uuid,
    pub user_id: Option<Uuid>,
    /// Student display name. **Masked** for non-admin / non-section-owner callers.
    pub user_display: String,
    pub user_email: Option<String>,
    pub section_id: Uuid,
    pub checkin_type: CheckinType,
    pub checked_in_at: NaiveDateTime,
    pub retry_sequence: i32,
    pub retry_of_id: Option<Uuid>,
    pub retry_reason: Option<String>,
    pub is_duplicate_attempt: bool,
    pub network_verified: bool,
    pub network_hint: Option<String>,
    pub client_ip: Option<String>,
    pub device_fingerprint: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CheckinResult {
    pub status: CheckinStatus,
    pub view: CheckinView,
    pub duplicate_window_minutes: u32,
    pub network_rule_active: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckinStatus {
    Success,
    Duplicate,
    Retried,
    NetworkBlocked,
}

#[derive(Debug, Clone, Serialize)]
pub struct RetryReason {
    pub reason_code: String,
    pub display_name: String,
    pub description: Option<String>,
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Record a one-tap check-in.
///
/// Flow:
///
/// 1. Capability + section visibility check.
/// 2. Network rule (IP/CIDR whitelist from `admin_settings`).
/// 3. Duplicate lookup in the configured time window. A positive match
///    writes a `is_duplicate_attempt = true` row for evidence and
///    returns [`CheckinStatus::Duplicate`] as `Err(Conflict)`.
/// 4. Insert the original attempt (`retry_sequence = 0`).
pub async fn check_in(
    pool: &MySqlPool,
    principal: &Principal,
    input: CheckinInput,
    client_ip: Option<&str>,
    user_agent: Option<&str>,
) -> AppResult<CheckinResult> {
    require(principal, Capability::CheckinWrite)?;

    // Section must exist and be readable to the caller.
    ensure_section_readable(pool, principal, input.section_id).await?;

    let cfg = load_checkin_config(pool).await?;

    // Network rule gate.
    let network_verified = verify_network_rule(&cfg, client_ip);

    // Duplicate window query.
    let since = Utc::now().naive_utc()
        - Duration::minutes(cfg.duplicate_window_minutes as i64);
    let duplicate = find_recent_attempt(pool, principal.user_id, input.section_id, since).await?;

    // If the network rule fails, persist the attempt with is_duplicate_attempt = true
    // so the evidence survives even when the server rejects the tap.
    if !network_verified {
        let id = insert_checkin_row(
            pool,
            principal.user_id,
            input.section_id,
            input.checkin_type,
            None,
            0,
            None,
            input.device_fingerprint.as_ref(),
            input.network_hint.as_deref(),
            network_verified,
            client_ip,
            /* is_duplicate */ false,
        )
        .await?;
        audit_service::record(
            pool,
            AuditEvent {
                actor_id: Some(principal.user_id),
                actor_email: Some(&principal.email),
                action: "checkin.network_blocked",
                target_entity_type: Some("checkin_event"),
                target_entity_id: Some(id),
                change_payload: Some(serde_json::json!({
                    "section_id": input.section_id,
                    "client_ip": client_ip,
                    "allowed_cidrs_count": cfg.allowed_cidrs.len(),
                })),
                ip_address: client_ip,
                user_agent,
            },
        )
        .await?;
        return Err(AppError::Forbidden);
    }

    // Duplicate within window?
    if let Some(existing) = duplicate {
        // Record the duplicate attempt as evidence.
        let id = insert_checkin_row(
            pool,
            principal.user_id,
            input.section_id,
            input.checkin_type,
            None,
            0,
            None,
            input.device_fingerprint.as_ref(),
            input.network_hint.as_deref(),
            network_verified,
            client_ip,
            /* is_duplicate */ true,
        )
        .await?;
        audit_service::record(
            pool,
            AuditEvent {
                actor_id: Some(principal.user_id),
                actor_email: Some(&principal.email),
                action: "checkin.duplicate",
                target_entity_type: Some("checkin_event"),
                target_entity_id: Some(id),
                change_payload: Some(serde_json::json!({
                    "section_id": input.section_id,
                    "original_id": existing,
                    "window_minutes": cfg.duplicate_window_minutes,
                })),
                ip_address: client_ip,
                user_agent,
            },
        )
        .await?;
        return Err(AppError::Conflict(format!(
            "duplicate check-in within {}-minute window — call the retry endpoint with a reason code if this was intentional",
            cfg.duplicate_window_minutes
        )));
    }

    // Happy path — insert the original attempt.
    let id = insert_checkin_row(
        pool,
        principal.user_id,
        input.section_id,
        input.checkin_type,
        None,
        0,
        None,
        input.device_fingerprint.as_ref(),
        input.network_hint.as_deref(),
        network_verified,
        client_ip,
        false,
    )
    .await?;

    audit_service::record(
        pool,
        AuditEvent {
            actor_id: Some(principal.user_id),
            actor_email: Some(&principal.email),
            action: "checkin.success",
            target_entity_type: Some("checkin_event"),
            target_entity_id: Some(id),
            change_payload: Some(serde_json::json!({
                "section_id": input.section_id,
                "checkin_type": input.checkin_type.as_db(),
            })),
            ip_address: client_ip,
            user_agent,
        },
    )
    .await?;

    let view = load_checkin_view(pool, principal, id).await?;
    Ok(CheckinResult {
        status: CheckinStatus::Success,
        view,
        duplicate_window_minutes: cfg.duplicate_window_minutes,
        network_rule_active: !cfg.allowed_cidrs.is_empty(),
    })
}

/// Single retry path. Accepts only controlled reason codes and enforces
/// the `checkin.max_retry_count` ceiling (default 1).
pub async fn retry_checkin(
    pool: &MySqlPool,
    principal: &Principal,
    original_id: Uuid,
    input: CheckinRetryInput,
    client_ip: Option<&str>,
    user_agent: Option<&str>,
) -> AppResult<CheckinResult> {
    require(principal, Capability::CheckinWrite)?;

    // 1. Reason code must exist.
    let reason_ok = sqlx::query(
        "SELECT 1 FROM checkin_retry_reasons WHERE reason_code = ? AND is_active = TRUE LIMIT 1",
    )
    .bind(&input.reason_code)
    .fetch_optional(pool)
    .await
    .map_err(|e| AppError::Database(format!("retry reason lookup: {}", e)))?;
    if reason_ok.is_none() {
        return Err(AppError::Validation(format!(
            "retry reason '{}' is not in the controlled list",
            input.reason_code
        )));
    }

    // 2. Load the original row — it must belong to the caller.
    let row = sqlx::query(
        r#"SELECT id, user_id, section_id, checkin_type
             FROM checkin_events
            WHERE id = ? AND is_duplicate_attempt = FALSE AND retry_sequence = 0"#,
    )
    .bind(original_id.to_string())
    .fetch_optional(pool)
    .await
    .map_err(|e| AppError::Database(format!("retry load: {}", e)))?
    .ok_or_else(|| AppError::NotFound(format!("original check-in {}", original_id)))?;

    let owner: String = row
        .try_get("user_id")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let section_id_s: String = row
        .try_get("section_id")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let checkin_type_s: String = row
        .try_get("checkin_type")
        .map_err(|e| AppError::Database(e.to_string()))?;

    if owner != principal.user_id.to_string() && !principal.is_admin() {
        return Err(AppError::Forbidden);
    }

    let section_id = Uuid::parse_str(&section_id_s)
        .map_err(|e| AppError::Database(e.to_string()))?;
    let checkin_type = CheckinType::from_db(&checkin_type_s)
        .unwrap_or(CheckinType::ManualInstructor);

    // 3. Retry count gate.
    let cfg = load_checkin_config(pool).await?;
    let existing_retries: i64 = sqlx::query(
        "SELECT COUNT(*) AS n FROM checkin_events WHERE retry_of_id = ? AND is_duplicate_attempt = FALSE",
    )
    .bind(original_id.to_string())
    .fetch_one(pool)
    .await
    .map_err(|e| AppError::Database(format!("retry count: {}", e)))?
    .try_get("n")
    .map_err(|e| AppError::Database(e.to_string()))?;
    if existing_retries as u32 >= cfg.max_retry_count {
        return Err(AppError::Conflict(format!(
            "maximum of {} retry already recorded for this check-in",
            cfg.max_retry_count
        )));
    }

    // INVARIANT: network block → Err(AppError::Forbidden) in BOTH check-in and retry paths.
    // Do NOT return Ok(…) or a 200 with status="network_blocked" — that was the pre-fix bug.
    // The retry slot must NOT be decremented when the rule fails; the caller should be able
    // to re-attempt once their device moves to an allowed network.
    // Regressions covered by:
    //   • db_retry_returns_403_on_network_failure_not_200_network_blocked (api_routes_test, test 15)
    //   • db_retry_slot_not_consumed_on_network_block (api_routes_test, test 18)
    //   • checkin_network_blocked_retry.sh (API_tests)
    // 4. Network rule re-evaluated for the retry — same hard-reject policy as the initial path.
    //    When the rule fails: audit the blocked attempt WITHOUT consuming the retry slot, then
    //    return 403.  The caller's retry count is unchanged so they may re-attempt once the
    //    network condition is resolved (e.g., the device moves back onto an allowed subnet).
    let network_verified = verify_network_rule(&cfg, client_ip);
    if !network_verified {
        // Persist a blocked-attempt row for audit evidence (mirrors initial check-in behaviour).
        let blocked_id = insert_checkin_row(
            pool,
            principal.user_id,
            section_id,
            checkin_type,
            Some(original_id),
            // Use retry_seq = existing_retries + 1 for ordering but do NOT count this slot;
            // the retry_count gate above already fired before we reach here, so this row is
            // evidence-only and is_duplicate_attempt = true prevents double-counting.
            (existing_retries as i32) + 1,
            Some(&input.reason_code),
            input.device_fingerprint.as_ref(),
            input.network_hint.as_deref(),
            /* network_verified */ false,
            client_ip,
            /* is_duplicate */ true,
        )
        .await?;
        audit_service::record(
            pool,
            AuditEvent {
                actor_id: Some(principal.user_id),
                actor_email: Some(&principal.email),
                action: "checkin.network_blocked",
                target_entity_type: Some("checkin_event"),
                target_entity_id: Some(blocked_id),
                change_payload: Some(serde_json::json!({
                    "context": "retry_attempt",
                    "original_id": original_id,
                    "reason_code": input.reason_code,
                    "client_ip": client_ip,
                    "allowed_cidrs_count": cfg.allowed_cidrs.len(),
                })),
                ip_address: client_ip,
                user_agent,
            },
        )
        .await?;
        return Err(AppError::Forbidden);
    }

    // 5. Network passed — insert the actual retry row (consumes the slot).
    let retry_seq = (existing_retries as i32) + 1;
    let id = insert_checkin_row(
        pool,
        principal.user_id,
        section_id,
        checkin_type,
        Some(original_id),
        retry_seq,
        Some(&input.reason_code),
        input.device_fingerprint.as_ref(),
        input.network_hint.as_deref(),
        /* network_verified */ true,
        client_ip,
        /* is_duplicate */ false,
    )
    .await?;

    audit_service::record(
        pool,
        AuditEvent {
            actor_id: Some(principal.user_id),
            actor_email: Some(&principal.email),
            action: "checkin.retry",
            target_entity_type: Some("checkin_event"),
            target_entity_id: Some(id),
            change_payload: Some(serde_json::json!({
                "original_id": original_id,
                "reason_code": input.reason_code,
                "retry_sequence": retry_seq,
            })),
            ip_address: client_ip,
            user_agent,
        },
    )
    .await?;

    let view = load_checkin_view(pool, principal, id).await?;
    Ok(CheckinResult {
        status: CheckinStatus::Retried,
        view,
        duplicate_window_minutes: cfg.duplicate_window_minutes,
        network_rule_active: !cfg.allowed_cidrs.is_empty(),
    })
}

pub async fn list_checkins_for_section(
    pool: &MySqlPool,
    principal: &Principal,
    section_id: Uuid,
    limit: u32,
    offset: u32,
) -> AppResult<Vec<CheckinView>> {
    require(principal, Capability::CheckinRead)?;
    ensure_section_readable(pool, principal, section_id).await?;
    let limit = limit.clamp(1, 500) as i64;
    let offset = offset as i64;

    let rows = sqlx::query(
        r#"SELECT id FROM checkin_events
            WHERE section_id = ?
            ORDER BY checked_in_at DESC
            LIMIT ? OFFSET ?"#,
    )
    .bind(section_id.to_string())
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await
    .map_err(|e| AppError::Database(format!("list_checkins: {}", e)))?;

    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let id: String = row
            .try_get("id")
            .map_err(|e| AppError::Database(e.to_string()))?;
        let cid = Uuid::parse_str(&id).map_err(|e| AppError::Database(e.to_string()))?;
        out.push(load_checkin_view(pool, principal, cid).await?);
    }
    Ok(out)
}

pub async fn list_retry_reasons(pool: &MySqlPool) -> AppResult<Vec<RetryReason>> {
    let rows = sqlx::query(
        "SELECT reason_code, display_name, description FROM checkin_retry_reasons WHERE is_active = TRUE ORDER BY display_name ASC",
    )
    .fetch_all(pool)
    .await
    .map_err(|e| AppError::Database(format!("list_retry_reasons: {}", e)))?;

    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let reason_code: String = row
            .try_get("reason_code")
            .map_err(|e| AppError::Database(e.to_string()))?;
        let display_name: String = row
            .try_get("display_name")
            .map_err(|e| AppError::Database(e.to_string()))?;
        let description: Option<String> = row
            .try_get("description")
            .map_err(|e| AppError::Database(e.to_string()))?;
        out.push(RetryReason {
            reason_code,
            display_name,
            description,
        });
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Network rule — the truthful implementation
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct CheckinConfig {
    duplicate_window_minutes: u32,
    max_retry_count: u32,
    allowed_cidrs: Vec<IpNet>,
}

async fn load_checkin_config(pool: &MySqlPool) -> AppResult<CheckinConfig> {
    let rows = sqlx::query(
        r#"SELECT setting_key, CAST(setting_value AS CHAR) AS v
             FROM admin_settings
            WHERE setting_key IN
                  ('checkin.duplicate_window_minutes',
                   'checkin.max_retry_count',
                   'checkin.allowed_client_cidrs')"#,
    )
    .fetch_all(pool)
    .await
    .map_err(|e| AppError::Database(format!("checkin cfg load: {}", e)))?;

    let mut duplicate_window_minutes: u32 = 10;
    let mut max_retry_count: u32 = 1;
    let mut allowed_cidrs: Vec<IpNet> = Vec::new();
    for row in rows {
        let key: String = row
            .try_get("setting_key")
            .map_err(|e| AppError::Database(e.to_string()))?;
        let v: String = row
            .try_get("v")
            .map_err(|e| AppError::Database(e.to_string()))?;
        match key.as_str() {
            "checkin.duplicate_window_minutes" => {
                duplicate_window_minutes = serde_json::from_str::<u32>(&v).unwrap_or(10);
            }
            "checkin.max_retry_count" => {
                max_retry_count = serde_json::from_str::<u32>(&v).unwrap_or(1);
            }
            "checkin.allowed_client_cidrs" => {
                let parsed: Vec<String> =
                    serde_json::from_str(&v).unwrap_or_default();
                for s in parsed {
                    if let Ok(net) = IpNet::from_str(&s) {
                        allowed_cidrs.push(net);
                    } else {
                        tracing::warn!(
                            "ignoring invalid CIDR in checkin.allowed_client_cidrs: {}",
                            s
                        );
                    }
                }
            }
            _ => {}
        }
    }
    Ok(CheckinConfig {
        duplicate_window_minutes,
        max_retry_count,
        allowed_cidrs,
    })
}

/// Returns `true` if `ip_str` parses as an `IpAddr` that falls into any
/// of the provided CIDRs. Invalid IPs (and empty input) are rejected.
pub fn ip_matches_any_cidr(ip_str: &str, cidrs: &[IpNet]) -> bool {
    let Ok(ip) = IpAddr::from_str(ip_str.trim()) else {
        return false;
    };
    cidrs.iter().any(|c| c.contains(&ip))
}

/// Shared network-rule evaluator used by both the initial and retry paths.
///
/// Returns `true` when:
/// * No CIDRs are configured (rule disabled), or
/// * `client_ip` is present and falls within at least one configured CIDR.
///
/// Returns `false` when CIDRs are configured but the IP is absent or outside
/// all ranges.  Both paths treat `false` as a hard rejection (HTTP 403).
pub fn verify_network_rule(cfg: &CheckinConfig, client_ip: Option<&str>) -> bool {
    if cfg.allowed_cidrs.is_empty() {
        return true;
    }
    match client_ip {
        Some(ip) => ip_matches_any_cidr(ip, &cfg.allowed_cidrs),
        None => false,
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

async fn ensure_section_readable(
    pool: &MySqlPool,
    principal: &Principal,
    section_id: Uuid,
) -> AppResult<()> {
    if principal.is_admin() {
        return Ok(());
    }
    let row = sqlx::query(
        r#"SELECT c.department_id
             FROM sections s
             JOIN courses c ON c.id = s.course_id
            WHERE s.id = ?"#,
    )
    .bind(section_id.to_string())
    .fetch_optional(pool)
    .await
    .map_err(|e| AppError::Database(format!("section read check: {}", e)))?
    .ok_or_else(|| AppError::NotFound(format!("section {}", section_id)))?;
    let dept: Option<String> = row
        .try_get("department_id")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let dept = dept.as_deref().and_then(|s| Uuid::parse_str(s).ok());

    // Librarian sees everything; everyone else is pinned to their dept.
    if principal.has_role(Role::Librarian) {
        return Ok(());
    }
    match (principal.department_id, dept) {
        (Some(caller), Some(sect_dept)) if caller == sect_dept => Ok(()),
        _ => Err(AppError::Forbidden),
    }
}

async fn find_recent_attempt(
    pool: &MySqlPool,
    user_id: Uuid,
    section_id: Uuid,
    since: NaiveDateTime,
) -> AppResult<Option<Uuid>> {
    let row = sqlx::query(
        r#"SELECT id FROM checkin_events
            WHERE user_id = ? AND section_id = ?
              AND is_duplicate_attempt = FALSE
              AND retry_sequence = 0
              AND network_verified = TRUE
              AND checked_in_at >= ?
            ORDER BY checked_in_at DESC
            LIMIT 1"#,
    )
    .bind(user_id.to_string())
    .bind(section_id.to_string())
    .bind(since)
    .fetch_optional(pool)
    .await
    .map_err(|e| AppError::Database(format!("duplicate lookup: {}", e)))?;
    let Some(row) = row else { return Ok(None) };
    let id: String = row.try_get("id").map_err(|e| AppError::Database(e.to_string()))?;
    Ok(Some(
        Uuid::parse_str(&id).map_err(|e| AppError::Database(e.to_string()))?,
    ))
}

#[allow(clippy::too_many_arguments)]
async fn insert_checkin_row(
    pool: &MySqlPool,
    user_id: Uuid,
    section_id: Uuid,
    checkin_type: CheckinType,
    retry_of_id: Option<Uuid>,
    retry_sequence: i32,
    retry_reason: Option<&str>,
    device_fingerprint: Option<&serde_json::Value>,
    network_hint: Option<&str>,
    network_verified: bool,
    client_ip: Option<&str>,
    is_duplicate: bool,
) -> AppResult<Uuid> {
    let id = Uuid::new_v4();
    let now = Utc::now().naive_utc();
    let event_date = now.date();
    let fingerprint_json = match device_fingerprint {
        Some(v) => serde_json::to_string(v)
            .map_err(|e| AppError::Internal(format!("serialize device fingerprint: {}", e)))?,
        None => "null".into(),
    };

    sqlx::query(
        r#"INSERT INTO checkin_events
           (id, user_id, section_id, checkin_type, checked_in_at, event_date,
            device_info, retry_of_id, retry_sequence, retry_reason,
            device_fingerprint, network_hint, network_verified, client_ip,
            is_duplicate_attempt)
           VALUES (?, ?, ?, ?, ?, ?, NULL, ?, ?, ?,
                   CAST(? AS JSON), ?, ?, ?, ?)"#,
    )
    .bind(id.to_string())
    .bind(user_id.to_string())
    .bind(section_id.to_string())
    .bind(checkin_type.as_db())
    .bind(now)
    .bind(event_date)
    .bind(retry_of_id.map(|u| u.to_string()))
    .bind(retry_sequence)
    .bind(retry_reason)
    .bind(&fingerprint_json)
    .bind(network_hint)
    .bind(network_verified)
    .bind(client_ip)
    .bind(is_duplicate)
    .execute(pool)
    .await
    .map_err(|e| AppError::Database(format!("insert checkin: {}", e)))?;
    Ok(id)
}

async fn load_checkin_view(
    pool: &MySqlPool,
    principal: &Principal,
    checkin_id: Uuid,
) -> AppResult<CheckinView> {
    let row = sqlx::query(
        r#"SELECT e.id, e.user_id, e.section_id, e.checkin_type,
                  e.checked_in_at, e.retry_sequence, e.retry_of_id,
                  e.retry_reason, e.is_duplicate_attempt, e.network_verified,
                  e.network_hint, e.client_ip,
                  CAST(e.device_fingerprint AS CHAR) AS fingerprint_text,
                  u.email, u.display_name,
                  c.department_id, s.instructor_id
             FROM checkin_events e
             JOIN users u     ON u.id = e.user_id
             JOIN sections s  ON s.id = e.section_id
             JOIN courses c   ON c.id = s.course_id
            WHERE e.id = ?"#,
    )
    .bind(checkin_id.to_string())
    .fetch_optional(pool)
    .await
    .map_err(|e| AppError::Database(format!("load checkin: {}", e)))?
    .ok_or_else(|| AppError::NotFound(format!("checkin {}", checkin_id)))?;

    let user_id_s: String = row
        .try_get("user_id")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let section_id: String = row
        .try_get("section_id")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let checkin_type_s: String = row
        .try_get("checkin_type")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let checked_in_at: NaiveDateTime = row
        .try_get("checked_in_at")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let retry_sequence: i32 = row
        .try_get("retry_sequence")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let retry_of_id: Option<String> = row
        .try_get("retry_of_id")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let retry_reason: Option<String> = row
        .try_get("retry_reason")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let is_duplicate_attempt: bool = row
        .try_get::<i8, _>("is_duplicate_attempt")
        .map(|v| v != 0)
        .map_err(|e| AppError::Database(e.to_string()))?;
    let network_verified: bool = row
        .try_get::<i8, _>("network_verified")
        .map(|v| v != 0)
        .map_err(|e| AppError::Database(e.to_string()))?;
    let network_hint: Option<String> = row
        .try_get("network_hint")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let client_ip: Option<String> = row
        .try_get("client_ip")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let fingerprint_text: Option<String> = row
        .try_get("fingerprint_text")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let email: String = row
        .try_get("email")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let display_name: String = row
        .try_get("display_name")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let dept_id: Option<String> = row
        .try_get("department_id")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let instr_id: Option<String> = row
        .try_get("instructor_id")
        .map_err(|e| AppError::Database(e.to_string()))?;

    let user_uuid = Uuid::parse_str(&user_id_s).ok();
    let dept_uuid = dept_id.as_deref().and_then(|s| Uuid::parse_str(s).ok());
    let instructor_uuid = instr_id.as_deref().and_then(|s| Uuid::parse_str(s).ok());

    // Masking: apply Phase 2's masking policy with a check-in-specific
    // twist — students only see their OWN rows de-masked.
    let can_see_pii = principal.is_admin()
        || (principal.has_role(Role::DepartmentHead) && dept_uuid == principal.department_id)
        || (principal.has_role(Role::Instructor) && instructor_uuid == Some(principal.user_id))
        || principal_can(principal, Capability::DashboardViewSensitive)
        || user_uuid == Some(principal.user_id);

    let (user_display, user_email) = if can_see_pii {
        (display_name.clone(), Some(email))
    } else {
        (
            super::masking::mask_email_for_audit(&email, principal),
            None,
        )
    };

    let fingerprint_value = fingerprint_text
        .as_deref()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok());

    Ok(CheckinView {
        id: checkin_id,
        user_id: if can_see_pii { user_uuid } else { None },
        user_display,
        user_email,
        section_id: Uuid::parse_str(&section_id)
            .map_err(|e| AppError::Database(e.to_string()))?,
        checkin_type: CheckinType::from_db(&checkin_type_s).unwrap_or(CheckinType::QrCode),
        checked_in_at,
        retry_sequence,
        retry_of_id: retry_of_id.as_deref().and_then(|s| Uuid::parse_str(s).ok()),
        retry_reason,
        is_duplicate_attempt,
        network_verified,
        network_hint,
        client_ip: if can_see_pii { client_ip } else { None },
        device_fingerprint: if can_see_pii { fingerprint_value } else { None },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cidr_match_v4() {
        let cidrs = vec![
            IpNet::from_str("10.0.0.0/8").unwrap(),
            IpNet::from_str("192.168.1.0/24").unwrap(),
        ];
        assert!(ip_matches_any_cidr("10.1.2.3", &cidrs));
        assert!(ip_matches_any_cidr("192.168.1.42", &cidrs));
        assert!(!ip_matches_any_cidr("192.168.2.42", &cidrs));
        assert!(!ip_matches_any_cidr("8.8.8.8", &cidrs));
    }

    #[test]
    fn cidr_match_v6() {
        let cidrs = vec![IpNet::from_str("fe80::/10").unwrap()];
        assert!(ip_matches_any_cidr("fe80::1", &cidrs));
        assert!(!ip_matches_any_cidr("2001:db8::1", &cidrs));
    }

    #[test]
    fn cidr_rejects_garbage_input() {
        let cidrs = vec![IpNet::from_str("10.0.0.0/8").unwrap()];
        assert!(!ip_matches_any_cidr("not-an-ip", &cidrs));
        assert!(!ip_matches_any_cidr("", &cidrs));
        assert!(!ip_matches_any_cidr("10.0.0.1; drop table", &cidrs));
    }

    #[test]
    fn cidr_empty_list_rejects_everything() {
        // An empty list means "no CIDRs allowed" at the helper level.
        // The service-level rule interprets an empty *admin_settings*
        // value as "rule disabled", which is a different code path —
        // see `check_in`.
        let cidrs: Vec<IpNet> = vec![];
        assert!(!ip_matches_any_cidr("10.0.0.1", &cidrs));
    }

    #[test]
    fn checkin_type_db_round_trip() {
        for t in [
            CheckinType::QrCode,
            CheckinType::Geofence,
            CheckinType::ManualInstructor,
            CheckinType::NfcBeacon,
        ] {
            assert_eq!(CheckinType::from_db(t.as_db()), Some(t));
        }
        assert_eq!(CheckinType::from_db("nope"), None);
    }

    // --- Phase 5 test coverage additions ---------------------------------

    /// A CIDR list mixing a /8 and a /24. Verifies the membership check
    /// honours each prefix independently and that an IP outside both is
    /// rejected. Also asserts the (intentional) overlap between 10.0.0.0/8
    /// and any /24 inside 10.0.0.0/8 still resolves as a membership hit
    /// because `.any()` short-circuits on the first match.
    #[test]
    fn cidr_v4_multi_range() {
        let cidrs = vec![
            IpNet::from_str("10.0.0.0/8").unwrap(),
            IpNet::from_str("10.1.2.0/24").unwrap(),
        ];

        // Both ranges match an IP inside the /24 (overlap is fine).
        assert!(ip_matches_any_cidr("10.1.2.5", &cidrs));
        // /8 matches.
        assert!(ip_matches_any_cidr("10.250.0.1", &cidrs));
        // Neither matches.
        assert!(!ip_matches_any_cidr("172.16.0.1", &cidrs));

        // Overlap detection — both CIDRs contain 10.1.2.5.
        let matching = cidrs
            .iter()
            .filter(|c| c.contains(&"10.1.2.5".parse::<IpAddr>().unwrap()))
            .count();
        assert_eq!(matching, 2, "both /8 and /24 should claim 10.1.2.5");
    }

    /// Invalid CIDR strings must fail to parse via IpNet::from_str, and
    /// a helper that filters invalid entries should produce an all-valid
    /// Vec<IpNet> that still enforces membership correctly.
    #[test]
    fn cidr_rejects_invalid_cidr_string() {
        assert!(IpNet::from_str("not-a-cidr").is_err());
        assert!(IpNet::from_str("999.999.999.999/8").is_err());
        assert!(IpNet::from_str("10.0.0.0/99").is_err());

        // Filter helper — simulates the load_checkin_config parsing loop.
        let raw = vec![
            "10.0.0.0/8".to_string(),
            "not-a-cidr".to_string(),
            "192.168.1.0/24".to_string(),
            "".to_string(),
        ];
        let valid: Vec<IpNet> = raw
            .iter()
            .filter_map(|s| IpNet::from_str(s).ok())
            .collect();
        assert_eq!(valid.len(), 2, "two invalid entries must be dropped");
        assert!(ip_matches_any_cidr("10.1.2.3", &valid));
        assert!(ip_matches_any_cidr("192.168.1.1", &valid));
        assert!(!ip_matches_any_cidr("8.8.8.8", &valid));
    }

    /// CheckinStatus is serialized as lowercase snake_case JSON. The
    /// wire format is part of the API contract — dashboards and the
    /// frontend assume these literals.
    #[test]
    fn checkin_status_serde_snake_case() {
        assert_eq!(
            serde_json::to_string(&CheckinStatus::Success).unwrap(),
            "\"success\""
        );
        assert_eq!(
            serde_json::to_string(&CheckinStatus::Duplicate).unwrap(),
            "\"duplicate\""
        );
        assert_eq!(
            serde_json::to_string(&CheckinStatus::Retried).unwrap(),
            "\"retried\""
        );
        assert_eq!(
            serde_json::to_string(&CheckinStatus::NetworkBlocked).unwrap(),
            "\"network_blocked\""
        );
    }

    /// Exhaustive round trip for every known CheckinType variant from the
    /// DB string form back to the enum. Guarantees that as_db and from_db
    /// agree for every variant.
    #[test]
    fn checkin_type_all_variants_from_db() {
        let all = [
            (CheckinType::QrCode, "qr_code"),
            (CheckinType::Geofence, "geofence"),
            (CheckinType::ManualInstructor, "manual_instructor"),
            (CheckinType::NfcBeacon, "nfc_beacon"),
        ];
        for (variant, db_name) in all {
            assert_eq!(variant.as_db(), db_name);
            assert_eq!(CheckinType::from_db(db_name), Some(variant));
        }
        assert_eq!(CheckinType::from_db(""), None);
        assert_eq!(CheckinType::from_db("QR_CODE"), None); // case-sensitive
    }

    // --- verify_network_rule unit tests -----------------------------------

    fn make_cfg(cidrs: &[&str]) -> CheckinConfig {
        CheckinConfig {
            duplicate_window_minutes: 30,
            max_retry_count: 1,
            allowed_cidrs: cidrs
                .iter()
                .filter_map(|s| IpNet::from_str(s).ok())
                .collect(),
        }
    }

    /// When no CIDRs are configured the rule is disabled — every call passes,
    /// regardless of whether a client IP is provided.
    #[test]
    fn verify_network_rule_disabled_when_no_cidrs() {
        let cfg = make_cfg(&[]);
        assert!(verify_network_rule(&cfg, Some("8.8.8.8")));
        assert!(verify_network_rule(&cfg, None));
    }

    /// An IP inside the configured range is accepted.
    #[test]
    fn verify_network_rule_passes_allowed_ip() {
        let cfg = make_cfg(&["10.0.0.0/8"]);
        assert!(verify_network_rule(&cfg, Some("10.1.2.3")));
    }

    /// An IP outside every configured range is rejected.
    #[test]
    fn verify_network_rule_rejects_disallowed_ip() {
        let cfg = make_cfg(&["10.0.0.0/8"]);
        assert!(!verify_network_rule(&cfg, Some("192.168.1.1")));
    }

    /// When CIDRs are active but no IP is present (e.g. proxy stripped it)
    /// the call is rejected — unknown origin is not trusted.
    #[test]
    fn verify_network_rule_rejects_missing_ip_when_rule_active() {
        let cfg = make_cfg(&["10.0.0.0/8"]);
        assert!(!verify_network_rule(&cfg, None));
    }

    /// Unified policy: both initial and retry paths use the same helper.
    /// Verify that the helper result is identical for the same inputs
    /// regardless of which call site invokes it.
    #[test]
    fn verify_network_rule_consistent_across_call_sites() {
        let cfg = make_cfg(&["192.168.0.0/16"]);
        let ip_in = "192.168.5.10";
        let ip_out = "10.0.0.1";

        // Simulated initial-path call.
        let initial_in = verify_network_rule(&cfg, Some(ip_in));
        let initial_out = verify_network_rule(&cfg, Some(ip_out));

        // Simulated retry-path call — must produce the same result.
        let retry_in = verify_network_rule(&cfg, Some(ip_in));
        let retry_out = verify_network_rule(&cfg, Some(ip_out));

        assert_eq!(initial_in, retry_in, "initial and retry must agree on allowed IP");
        assert_eq!(initial_out, retry_out, "initial and retry must agree on blocked IP");
        assert!(initial_in, "allowed IP must pass");
        assert!(!initial_out, "blocked IP must fail");
    }
}
