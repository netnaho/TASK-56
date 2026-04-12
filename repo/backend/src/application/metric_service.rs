//! Metric semantic layer with versioning, lineage, and dependent-widget
//! verification flags.
//!
//! # Workflow
//!
//! Same lifecycle as journals/courses/resources:
//! `draft → approved → published → archived` with a
//! `current_version_id` baseline and a `latest_version_id` head
//! pointer on the parent `metric_definitions` row.
//!
//! # Lineage
//!
//! A `metric_definition_version` declares its input metrics as a JSON
//! array of `{definition_id, version_id}` objects in `lineage_refs`.
//! Any widget that consumes this metric tracks the version it was
//! last verified against (`dashboard_widgets.based_on_version_id`);
//! when a new version is **published**, the widget is automatically
//! marked `verification_needed = TRUE` so a human must re-review the
//! chart before it leaves the dashboard.
//!
//! # Admin review requirement
//!
//! `publish_version` is gated by `Capability::MetricApprove` which
//! Phase 5 grants only to `Role::Admin`. Department heads and
//! librarians can create and edit metric definitions but cannot
//! publish their own changes.

use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use sqlx::{MySqlPool, Row};
use uuid::Uuid;

use super::audit_service::{self, AuditEvent};
use super::authorization::{require, Capability};
use super::principal::Principal;
use crate::domain::versioning::{validate_transition, VersionState};
use crate::errors::{AppError, AppResult};

// ---------------------------------------------------------------------------
// Inputs and view models
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MetricType {
    Base,
    Derived,
}

impl MetricType {
    pub fn as_db(self) -> &'static str {
        match self {
            MetricType::Base => "base",
            MetricType::Derived => "derived",
        }
    }
    pub fn from_db(s: &str) -> Option<Self> {
        match s {
            "base" => Some(MetricType::Base),
            "derived" => Some(MetricType::Derived),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Polarity {
    HigherIsBetter,
    LowerIsBetter,
    Neutral,
}

impl Polarity {
    pub fn as_db(self) -> &'static str {
        match self {
            Polarity::HigherIsBetter => "higher_is_better",
            Polarity::LowerIsBetter => "lower_is_better",
            Polarity::Neutral => "neutral",
        }
    }
    pub fn from_db(s: &str) -> Option<Self> {
        match s {
            "higher_is_better" => Some(Polarity::HigherIsBetter),
            "lower_is_better" => Some(Polarity::LowerIsBetter),
            "neutral" => Some(Polarity::Neutral),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LineageRef {
    pub definition_id: Uuid,
    pub version_id: Uuid,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MetricCreateInput {
    pub key_name: String,
    pub display_name: String,
    pub unit: Option<String>,
    pub polarity: Polarity,
    pub formula: String,
    pub description: Option<String>,
    pub metric_type: MetricType,
    pub window_seconds: Option<i32>,
    pub change_summary: Option<String>,
    #[serde(default)]
    pub lineage_refs: Vec<LineageRef>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MetricEditInput {
    pub formula: String,
    pub description: Option<String>,
    pub metric_type: MetricType,
    pub window_seconds: Option<i32>,
    pub change_summary: Option<String>,
    #[serde(default)]
    pub lineage_refs: Vec<LineageRef>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MetricDefinitionView {
    pub id: Uuid,
    pub key_name: String,
    pub display_name: String,
    pub unit: Option<String>,
    pub polarity: Polarity,
    pub current_version_id: Option<Uuid>,
    pub latest_version_id: Option<Uuid>,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
    pub effective_version: Option<MetricVersionView>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MetricVersionView {
    pub id: Uuid,
    pub metric_definition_id: Uuid,
    pub version_number: i32,
    pub formula: String,
    pub description: Option<String>,
    pub metric_type: MetricType,
    pub window_seconds: Option<i32>,
    pub lineage_refs: Vec<LineageRef>,
    pub change_summary: Option<String>,
    pub state: VersionState,
    pub created_by: Option<Uuid>,
    pub created_at: NaiveDateTime,
    pub approved_at: Option<NaiveDateTime>,
    pub published_at: Option<NaiveDateTime>,
}

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

const KEY_MAX: usize = 255;
const NAME_MAX: usize = 255;
const FORMULA_MAX: usize = 10_000;
const DESCRIPTION_MAX: usize = 4_000;
const CHANGE_SUMMARY_MAX: usize = 2_000;

fn is_valid_key(key: &str) -> bool {
    let t = key.trim();
    if t.is_empty() || t.len() > KEY_MAX {
        return false;
    }
    t.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.')
}

fn validate_inputs(
    key_name: Option<&str>,
    display_name: Option<&str>,
    formula: &str,
    description: Option<&str>,
    change_summary: Option<&str>,
) -> AppResult<()> {
    if let Some(k) = key_name {
        if !is_valid_key(k) {
            return Err(AppError::Validation(
                "key_name must be snake.dot_case and <= 255 chars".into(),
            ));
        }
    }
    if let Some(d) = display_name {
        let n = d.trim().chars().count();
        if n < 3 || n > NAME_MAX {
            return Err(AppError::Validation(format!(
                "display_name must be 3-{} chars",
                NAME_MAX
            )));
        }
    }
    if formula.trim().is_empty() {
        return Err(AppError::Validation("formula must not be empty".into()));
    }
    if formula.chars().count() > FORMULA_MAX {
        return Err(AppError::Validation(format!(
            "formula exceeds {} chars",
            FORMULA_MAX
        )));
    }
    if let Some(v) = description {
        if v.chars().count() > DESCRIPTION_MAX {
            return Err(AppError::Validation("description too long".into()));
        }
    }
    if let Some(v) = change_summary {
        if v.chars().count() > CHANGE_SUMMARY_MAX {
            return Err(AppError::Validation("change_summary too long".into()));
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

pub async fn create_metric(
    pool: &MySqlPool,
    principal: &Principal,
    input: MetricCreateInput,
) -> AppResult<MetricDefinitionView> {
    require(principal, Capability::MetricWrite)?;
    validate_inputs(
        Some(&input.key_name),
        Some(&input.display_name),
        &input.formula,
        input.description.as_deref(),
        input.change_summary.as_deref(),
    )?;

    // Key uniqueness.
    let existing = sqlx::query("SELECT 1 FROM metric_definitions WHERE key_name = ? LIMIT 1")
        .bind(input.key_name.trim())
        .fetch_optional(pool)
        .await
        .map_err(|e| AppError::Database(format!("metric key check: {}", e)))?;
    if existing.is_some() {
        return Err(AppError::Conflict(format!(
            "metric key '{}' already exists",
            input.key_name.trim()
        )));
    }

    validate_lineage_refs(pool, &input.lineage_refs).await?;

    let mut tx = pool
        .begin()
        .await
        .map_err(|e| AppError::Database(format!("create_metric tx: {}", e)))?;

    let metric_id = Uuid::new_v4();
    let version_id = Uuid::new_v4();
    let lineage_json = serde_json::to_string(&input.lineage_refs)
        .map_err(|e| AppError::Internal(format!("lineage serialize: {}", e)))?;

    sqlx::query(
        r#"INSERT INTO metric_definitions
           (id, key_name, display_name, unit, polarity, created_by)
           VALUES (?, ?, ?, ?, ?, ?)"#,
    )
    .bind(metric_id.to_string())
    .bind(input.key_name.trim())
    .bind(input.display_name.trim())
    .bind(input.unit.as_deref())
    .bind(input.polarity.as_db())
    .bind(principal.user_id.to_string())
    .execute(&mut *tx)
    .await
    .map_err(|e| AppError::Database(format!("metric insert: {}", e)))?;

    sqlx::query(
        r#"INSERT INTO metric_definition_versions
           (id, metric_definition_id, version_number, formula, metric_type,
            window_seconds, lineage_refs, description, change_summary, state, created_by)
           VALUES (?, ?, 1, ?, ?, ?, CAST(? AS JSON), ?, ?, 'draft', ?)"#,
    )
    .bind(version_id.to_string())
    .bind(metric_id.to_string())
    .bind(&input.formula)
    .bind(input.metric_type.as_db())
    .bind(input.window_seconds)
    .bind(&lineage_json)
    .bind(input.description.as_deref())
    .bind(input.change_summary.as_deref())
    .bind(principal.user_id.to_string())
    .execute(&mut *tx)
    .await
    .map_err(|e| AppError::Database(format!("metric version insert: {}", e)))?;

    sqlx::query("UPDATE metric_definitions SET latest_version_id = ? WHERE id = ?")
        .bind(version_id.to_string())
        .bind(metric_id.to_string())
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::Database(format!("update latest: {}", e)))?;

    tx.commit()
        .await
        .map_err(|e| AppError::Database(format!("create_metric commit: {}", e)))?;

    audit_service::record(
        pool,
        AuditEvent {
            actor_id: Some(principal.user_id),
            actor_email: Some(&principal.email),
            action: "metric.create",
            target_entity_type: Some("metric_definition"),
            target_entity_id: Some(metric_id),
            change_payload: Some(serde_json::json!({
                "key_name": input.key_name.trim(),
                "display_name": input.display_name.trim(),
                "metric_type": input.metric_type.as_db(),
                "lineage_refs": input.lineage_refs,
            })),
            ip_address: None,
            user_agent: None,
        },
    )
    .await?;

    get_metric_by_id(pool, principal, metric_id).await
}

pub async fn create_draft_version(
    pool: &MySqlPool,
    principal: &Principal,
    metric_id: Uuid,
    input: MetricEditInput,
) -> AppResult<MetricVersionView> {
    require(principal, Capability::MetricWrite)?;
    validate_inputs(
        None,
        None,
        &input.formula,
        input.description.as_deref(),
        input.change_summary.as_deref(),
    )?;
    validate_lineage_refs(pool, &input.lineage_refs).await?;

    let mut tx = pool
        .begin()
        .await
        .map_err(|e| AppError::Database(format!("draft tx: {}", e)))?;

    let max_row = sqlx::query(
        r#"SELECT COALESCE(MAX(version_number), 0) AS max_ver
             FROM metric_definition_versions
            WHERE metric_definition_id = ?
            FOR UPDATE"#,
    )
    .bind(metric_id.to_string())
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| AppError::Database(format!("max ver: {}", e)))?;
    let max_ver: i32 = max_row
        .try_get("max_ver")
        .map_err(|e| AppError::Database(e.to_string()))?;
    if max_ver == 0 {
        return Err(AppError::NotFound(format!("metric_definition {}", metric_id)));
    }
    let new_ver = max_ver + 1;
    let version_id = Uuid::new_v4();
    let lineage_json = serde_json::to_string(&input.lineage_refs)
        .map_err(|e| AppError::Internal(format!("lineage serialize: {}", e)))?;

    sqlx::query(
        r#"INSERT INTO metric_definition_versions
           (id, metric_definition_id, version_number, formula, metric_type,
            window_seconds, lineage_refs, description, change_summary, state, created_by)
           VALUES (?, ?, ?, ?, ?, ?, CAST(? AS JSON), ?, ?, 'draft', ?)"#,
    )
    .bind(version_id.to_string())
    .bind(metric_id.to_string())
    .bind(new_ver)
    .bind(&input.formula)
    .bind(input.metric_type.as_db())
    .bind(input.window_seconds)
    .bind(&lineage_json)
    .bind(input.description.as_deref())
    .bind(input.change_summary.as_deref())
    .bind(principal.user_id.to_string())
    .execute(&mut *tx)
    .await
    .map_err(|e| AppError::Database(format!("draft insert: {}", e)))?;

    sqlx::query("UPDATE metric_definitions SET latest_version_id = ? WHERE id = ?")
        .bind(version_id.to_string())
        .bind(metric_id.to_string())
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::Database(format!("update latest: {}", e)))?;

    tx.commit()
        .await
        .map_err(|e| AppError::Database(format!("draft commit: {}", e)))?;

    audit_service::record(
        pool,
        AuditEvent {
            actor_id: Some(principal.user_id),
            actor_email: Some(&principal.email),
            action: "metric.draft.create",
            target_entity_type: Some("metric_definition_version"),
            target_entity_id: Some(version_id),
            change_payload: Some(serde_json::json!({
                "metric_definition_id": metric_id,
                "version_number": new_ver,
                "lineage_refs": input.lineage_refs,
            })),
            ip_address: None,
            user_agent: None,
        },
    )
    .await?;

    load_version_by_id(pool, version_id).await
}

pub async fn approve_version(
    pool: &MySqlPool,
    principal: &Principal,
    metric_id: Uuid,
    version_id: Uuid,
) -> AppResult<MetricVersionView> {
    require(principal, Capability::MetricWrite)?;
    let existing = load_version_by_id(pool, version_id).await?;
    if existing.metric_definition_id != metric_id {
        return Err(AppError::NotFound(format!(
            "metric_definition_version {}",
            version_id
        )));
    }
    validate_transition(existing.state, VersionState::Approved)?;

    sqlx::query(
        r#"UPDATE metric_definition_versions
              SET state='approved', approved_by=?, approved_at=NOW()
            WHERE id = ?"#,
    )
    .bind(principal.user_id.to_string())
    .bind(version_id.to_string())
    .execute(pool)
    .await
    .map_err(|e| AppError::Database(format!("approve: {}", e)))?;

    audit_service::record(
        pool,
        AuditEvent {
            actor_id: Some(principal.user_id),
            actor_email: Some(&principal.email),
            action: "metric.approve",
            target_entity_type: Some("metric_definition_version"),
            target_entity_id: Some(version_id),
            change_payload: Some(serde_json::json!({ "metric_definition_id": metric_id })),
            ip_address: None,
            user_agent: None,
        },
    )
    .await?;
    load_version_by_id(pool, version_id).await
}

/// Publish a metric version, flag every dependent widget for review,
/// and update the parent baseline pointer — all inside a single tx.
pub async fn publish_version(
    pool: &MySqlPool,
    principal: &Principal,
    metric_id: Uuid,
    version_id: Uuid,
) -> AppResult<MetricDefinitionView> {
    require(principal, Capability::MetricApprove)?;

    let mut tx = pool
        .begin()
        .await
        .map_err(|e| AppError::Database(format!("publish tx: {}", e)))?;

    let row = sqlx::query(
        r#"SELECT state, metric_definition_id
             FROM metric_definition_versions
            WHERE id=? FOR UPDATE"#,
    )
    .bind(version_id.to_string())
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| AppError::Database(format!("publish load: {}", e)))?
    .ok_or_else(|| AppError::NotFound(format!("metric_version {}", version_id)))?;

    let state_s: String = row
        .try_get("state")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let mid_s: String = row
        .try_get("metric_definition_id")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let current = VersionState::from_db(&state_s).unwrap_or(VersionState::Draft);
    if mid_s != metric_id.to_string() {
        return Err(AppError::NotFound(format!("metric_version {}", version_id)));
    }
    validate_transition(current, VersionState::Published)?;

    sqlx::query(
        r#"UPDATE metric_definition_versions
              SET state='archived'
            WHERE metric_definition_id = ? AND state='published' AND id <> ?"#,
    )
    .bind(metric_id.to_string())
    .bind(version_id.to_string())
    .execute(&mut *tx)
    .await
    .map_err(|e| AppError::Database(format!("archive prior: {}", e)))?;

    sqlx::query(
        r#"UPDATE metric_definition_versions
              SET state='published', published_by=?, published_at=NOW()
            WHERE id=?"#,
    )
    .bind(principal.user_id.to_string())
    .bind(version_id.to_string())
    .execute(&mut *tx)
    .await
    .map_err(|e| AppError::Database(format!("publish update: {}", e)))?;

    sqlx::query("UPDATE metric_definitions SET current_version_id=? WHERE id=?")
        .bind(version_id.to_string())
        .bind(metric_id.to_string())
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::Database(format!("set baseline: {}", e)))?;

    // Flag every dependent widget for re-verification — except those
    // already pinned to this exact version (e.g. a re-publish).
    let flagged = sqlx::query(
        r#"UPDATE dashboard_widgets
              SET verification_needed = TRUE,
                  verified_by = NULL,
                  verified_at = NULL
            WHERE metric_definition_id = ?
              AND (based_on_version_id IS NULL OR based_on_version_id <> ?)"#,
    )
    .bind(metric_id.to_string())
    .bind(version_id.to_string())
    .execute(&mut *tx)
    .await
    .map_err(|e| AppError::Database(format!("flag widgets: {}", e)))?;

    tx.commit()
        .await
        .map_err(|e| AppError::Database(format!("publish commit: {}", e)))?;

    audit_service::record(
        pool,
        AuditEvent {
            actor_id: Some(principal.user_id),
            actor_email: Some(&principal.email),
            action: "metric.publish",
            target_entity_type: Some("metric_definition_version"),
            target_entity_id: Some(version_id),
            change_payload: Some(serde_json::json!({
                "metric_definition_id": metric_id,
                "widgets_flagged_for_review": flagged.rows_affected(),
            })),
            ip_address: None,
            user_agent: None,
        },
    )
    .await?;

    get_metric_by_id(pool, principal, metric_id).await
}

/// Clear the `verification_needed` flag for a single widget. Records
/// the approver and timestamp. Requires `MetricApprove`.
pub async fn mark_widget_verified(
    pool: &MySqlPool,
    principal: &Principal,
    widget_id: Uuid,
) -> AppResult<()> {
    require(principal, Capability::MetricApprove)?;
    let result = sqlx::query(
        r#"UPDATE dashboard_widgets
              SET verification_needed = FALSE,
                  verified_by = ?,
                  verified_at = NOW(),
                  based_on_version_id = (
                      SELECT md.current_version_id
                        FROM metric_definitions md
                       WHERE md.id = dashboard_widgets.metric_definition_id
                  )
            WHERE id = ?"#,
    )
    .bind(principal.user_id.to_string())
    .bind(widget_id.to_string())
    .execute(pool)
    .await
    .map_err(|e| AppError::Database(format!("verify widget: {}", e)))?;
    if result.rows_affected() == 0 {
        return Err(AppError::NotFound(format!("dashboard_widget {}", widget_id)));
    }

    audit_service::record(
        pool,
        AuditEvent {
            actor_id: Some(principal.user_id),
            actor_email: Some(&principal.email),
            action: "metric.widget.verify",
            target_entity_type: Some("dashboard_widget"),
            target_entity_id: Some(widget_id),
            change_payload: None,
            ip_address: None,
            user_agent: None,
        },
    )
    .await?;
    Ok(())
}

pub async fn list_metrics(
    pool: &MySqlPool,
    principal: &Principal,
    limit: u32,
    offset: u32,
) -> AppResult<Vec<MetricDefinitionView>> {
    require(principal, Capability::MetricRead)?;
    let limit = limit.clamp(1, 500) as i64;
    let offset = offset as i64;
    let rows = sqlx::query(
        "SELECT id FROM metric_definitions ORDER BY key_name ASC LIMIT ? OFFSET ?",
    )
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await
    .map_err(|e| AppError::Database(format!("list_metrics: {}", e)))?;
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let id: String = row
            .try_get("id")
            .map_err(|e| AppError::Database(e.to_string()))?;
        let mid = Uuid::parse_str(&id).map_err(|e| AppError::Database(e.to_string()))?;
        out.push(get_metric_by_id(pool, principal, mid).await?);
    }
    Ok(out)
}

pub async fn get_metric_by_id(
    pool: &MySqlPool,
    principal: &Principal,
    metric_id: Uuid,
) -> AppResult<MetricDefinitionView> {
    require(principal, Capability::MetricRead)?;
    let row = sqlx::query(
        r#"SELECT id, key_name, display_name, unit, polarity,
                  current_version_id, latest_version_id, created_at, updated_at
             FROM metric_definitions WHERE id = ?"#,
    )
    .bind(metric_id.to_string())
    .fetch_optional(pool)
    .await
    .map_err(|e| AppError::Database(format!("get_metric: {}", e)))?
    .ok_or_else(|| AppError::NotFound(format!("metric_definition {}", metric_id)))?;

    let key_name: String = row
        .try_get("key_name")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let display_name: String = row
        .try_get("display_name")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let unit: Option<String> = row
        .try_get("unit")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let polarity_s: String = row
        .try_get("polarity")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let current_version_id: Option<String> = row
        .try_get("current_version_id")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let latest_version_id: Option<String> = row
        .try_get("latest_version_id")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let created_at: NaiveDateTime = row
        .try_get("created_at")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let updated_at: NaiveDateTime = row
        .try_get("updated_at")
        .map_err(|e| AppError::Database(e.to_string()))?;

    let effective_id = latest_version_id
        .clone()
        .or_else(|| current_version_id.clone());
    let effective_version = match effective_id {
        Some(ref s) => {
            let id = Uuid::parse_str(s).map_err(|e| AppError::Database(e.to_string()))?;
            Some(load_version_by_id(pool, id).await?)
        }
        None => None,
    };

    Ok(MetricDefinitionView {
        id: metric_id,
        key_name,
        display_name,
        unit,
        polarity: Polarity::from_db(&polarity_s).unwrap_or(Polarity::Neutral),
        current_version_id: current_version_id
            .as_deref()
            .and_then(|s| Uuid::parse_str(s).ok()),
        latest_version_id: latest_version_id
            .as_deref()
            .and_then(|s| Uuid::parse_str(s).ok()),
        created_at,
        updated_at,
        effective_version,
    })
}

pub async fn list_versions(
    pool: &MySqlPool,
    principal: &Principal,
    metric_id: Uuid,
) -> AppResult<Vec<MetricVersionView>> {
    require(principal, Capability::MetricRead)?;
    let rows = sqlx::query(
        r#"SELECT id FROM metric_definition_versions
            WHERE metric_definition_id = ?
            ORDER BY version_number DESC"#,
    )
    .bind(metric_id.to_string())
    .fetch_all(pool)
    .await
    .map_err(|e| AppError::Database(format!("list_versions: {}", e)))?;
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let id: String = row
            .try_get("id")
            .map_err(|e| AppError::Database(e.to_string()))?;
        let vid = Uuid::parse_str(&id).map_err(|e| AppError::Database(e.to_string()))?;
        out.push(load_version_by_id(pool, vid).await?);
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

async fn validate_lineage_refs(pool: &MySqlPool, refs: &[LineageRef]) -> AppResult<()> {
    for r in refs {
        let exists = sqlx::query(
            r#"SELECT 1 FROM metric_definition_versions v
                JOIN metric_definitions d ON d.id = v.metric_definition_id
               WHERE d.id = ? AND v.id = ? LIMIT 1"#,
        )
        .bind(r.definition_id.to_string())
        .bind(r.version_id.to_string())
        .fetch_optional(pool)
        .await
        .map_err(|e| AppError::Database(format!("lineage check: {}", e)))?;
        if exists.is_none() {
            return Err(AppError::Validation(format!(
                "lineage ref ({}, {}) does not exist",
                r.definition_id, r.version_id
            )));
        }
    }
    Ok(())
}

async fn load_version_by_id(
    pool: &MySqlPool,
    version_id: Uuid,
) -> AppResult<MetricVersionView> {
    let row = sqlx::query(
        r#"SELECT id, metric_definition_id, version_number, formula, metric_type,
                  window_seconds, CAST(lineage_refs AS CHAR) AS lineage_text,
                  description, change_summary, state, created_by, created_at,
                  approved_at, published_at
             FROM metric_definition_versions
            WHERE id = ?"#,
    )
    .bind(version_id.to_string())
    .fetch_optional(pool)
    .await
    .map_err(|e| AppError::Database(format!("load_version: {}", e)))?
    .ok_or_else(|| AppError::NotFound(format!("metric_definition_version {}", version_id)))?;

    let id: String = row.try_get("id").map_err(|e| AppError::Database(e.to_string()))?;
    let metric_definition_id: String = row
        .try_get("metric_definition_id")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let version_number: i32 = row
        .try_get("version_number")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let formula: String = row
        .try_get("formula")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let metric_type_s: String = row
        .try_get("metric_type")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let window_seconds: Option<i32> = row
        .try_get("window_seconds")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let lineage_text: Option<String> = row
        .try_get("lineage_text")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let description: Option<String> = row
        .try_get("description")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let change_summary: Option<String> = row
        .try_get("change_summary")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let state_s: String = row
        .try_get("state")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let created_by: Option<String> = row
        .try_get("created_by")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let created_at: NaiveDateTime = row
        .try_get("created_at")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let approved_at: Option<NaiveDateTime> = row
        .try_get("approved_at")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let published_at: Option<NaiveDateTime> = row
        .try_get("published_at")
        .map_err(|e| AppError::Database(e.to_string()))?;

    let lineage_refs: Vec<LineageRef> = lineage_text
        .as_deref()
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or_default();

    Ok(MetricVersionView {
        id: Uuid::parse_str(&id).map_err(|e| AppError::Database(e.to_string()))?,
        metric_definition_id: Uuid::parse_str(&metric_definition_id)
            .map_err(|e| AppError::Database(e.to_string()))?,
        version_number,
        formula,
        description,
        metric_type: MetricType::from_db(&metric_type_s).unwrap_or(MetricType::Base),
        window_seconds,
        lineage_refs,
        change_summary,
        state: VersionState::from_db(&state_s).unwrap_or(VersionState::Draft),
        created_by: created_by.as_deref().and_then(|s| Uuid::parse_str(s).ok()),
        created_at,
        approved_at,
        published_at,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_name_validation() {
        assert!(is_valid_key("checkins.total"));
        assert!(is_valid_key("foot_traffic"));
        assert!(is_valid_key("A_B.c_3"));
        assert!(!is_valid_key(""));
        assert!(!is_valid_key("has space"));
        assert!(!is_valid_key("has-dash"));
        assert!(!is_valid_key("has/slash"));
    }

    #[test]
    fn metric_type_round_trip() {
        assert_eq!(MetricType::from_db("base"), Some(MetricType::Base));
        assert_eq!(MetricType::from_db("derived"), Some(MetricType::Derived));
        assert_eq!(MetricType::from_db("nope"), None);
    }

    #[test]
    fn polarity_round_trip() {
        for p in [
            Polarity::HigherIsBetter,
            Polarity::LowerIsBetter,
            Polarity::Neutral,
        ] {
            assert_eq!(Polarity::from_db(p.as_db()), Some(p));
        }
    }

    #[test]
    fn validate_inputs_rejects_empty_formula() {
        let err = validate_inputs(None, None, "", None, None).unwrap_err();
        match err {
            AppError::Validation(msg) => assert!(msg.contains("formula")),
            _ => panic!("expected Validation"),
        }
    }

    // --- Phase 5 test coverage additions ---------------------------------

    /// Polarity::from_db must return None for unknown variants rather
    /// than silently defaulting to Neutral.
    #[test]
    fn polarity_missing_variant_returns_none() {
        assert_eq!(Polarity::from_db("gibberish"), None);
        assert_eq!(Polarity::from_db(""), None);
        assert_eq!(Polarity::from_db("HIGHER_IS_BETTER"), None); // case-sensitive
    }

    /// A formula with exactly 10_001 characters must be rejected. The
    /// boundary is strict: FORMULA_MAX = 10_000.
    #[test]
    fn validate_inputs_rejects_long_formula() {
        let long = "x".repeat(10_001);
        let err = validate_inputs(None, None, &long, None, None).unwrap_err();
        match err {
            AppError::Validation(msg) => assert!(
                msg.contains("formula") && msg.contains("10000"),
                "expected formula length error, got {msg}"
            ),
            _ => panic!("expected Validation"),
        }
    }

    /// Exactly 10_000 characters is the boundary — it must be accepted.
    #[test]
    fn validate_inputs_accepts_at_exact_limit() {
        let at_limit = "x".repeat(10_000);
        assert!(
            validate_inputs(None, None, &at_limit, None, None).is_ok(),
            "formula at the exact FORMULA_MAX boundary must be accepted"
        );
    }

    /// Covers several edge cases around the key_name validator.
    ///
    /// * Single-char keys are allowed.
    /// * Dotted keys stay valid.
    /// * Mixed case + digits are accepted.
    /// * Dashes are rejected (the validator only allows alphanumerics,
    ///   underscore, and dot).
    #[test]
    fn key_name_edge_cases() {
        assert!(is_valid_key("a"));
        assert!(is_valid_key("a.b.c"));
        assert!(is_valid_key("A1.b2"));
        assert!(!is_valid_key("--dash"));
        assert!(!is_valid_key("starts.with.-"));
    }
}
