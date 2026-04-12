//! Typed wrappers for the Phase 6 data-retention endpoints.
//!
//! Retention policies define how long each entity type is kept before
//! being deleted, anonymized, or flagged. This module exposes list,
//! update, and execution helpers.

use serde::{Deserialize, Serialize};

use crate::api::client::{ApiClient, ApiError};

// ── Wire types ───────────────────────────────────────────────────────────────

/// A single retention policy as returned by the backend.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct RetentionPolicyView {
    pub id: String,
    pub target_entity_type: String,
    pub retention_days: i32,
    pub action: String,
    #[serde(default)]
    pub rationale: Option<String>,
    pub is_active: bool,
    #[serde(default)]
    pub created_by: Option<String>,
    #[serde(default)]
    pub last_executed_at: Option<String>,
    #[serde(default)]
    pub last_execution_result: Option<String>,
    #[serde(default)]
    pub eligible_rows: Option<i64>,
    pub created_at: String,
    pub updated_at: String,
}

/// Per-policy result inside a [`RetentionExecutionSummary`].
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct RetentionExecutionResult {
    pub policy_id: String,
    pub target_entity_type: String,
    pub action: String,
    pub rows_affected: i64,
    #[serde(default)]
    pub files_deleted: Option<i64>,
    pub dry_run: bool,
    #[serde(default)]
    pub error: Option<String>,
    pub executed_at: String,
}

/// Aggregate summary returned when executing all policies.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct RetentionExecutionSummary {
    pub policies_run: i32,
    pub policies_skipped: i32,
    pub total_rows_affected: i64,
    #[serde(default)]
    pub total_files_deleted: Option<i64>,
    pub dry_run: bool,
    pub results: Vec<RetentionExecutionResult>,
    pub executed_at: String,
}

// ── Input types ──────────────────────────────────────────────────────────────

/// Partial-update body accepted by `PUT /retention/policies/<id>`.
#[derive(Debug, Clone, Serialize, Default)]
pub struct UpdateRetentionPolicyInput {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retention_days: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rationale: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_active: Option<bool>,
}

// ── Helper ───────────────────────────────────────────────────────────────────

fn client(token: &str) -> ApiClient {
    ApiClient::new(Some(token.to_string()))
}

// ── API functions ─────────────────────────────────────────────────────────────

/// Lists all retention policies.
pub async fn list_policies(token: &str) -> Result<Vec<RetentionPolicyView>, ApiError> {
    client(token).get_json("/retention/policies").await
}

/// Partially updates a retention policy (retention_days, action,
/// rationale, is_active).
pub async fn update_policy(
    token: &str,
    id: &str,
    input: &UpdateRetentionPolicyInput,
) -> Result<RetentionPolicyView, ApiError> {
    let path = format!("/retention/policies/{}", id);
    client(token).put_json(&path, input).await
}

/// Executes all active retention policies. Pass `dry_run = true` to
/// preview the effect without committing any changes.
pub async fn execute_all(
    token: &str,
    dry_run: bool,
) -> Result<RetentionExecutionSummary, ApiError> {
    let path = format!("/retention/execute?dry_run={}", dry_run);
    client(token).post_no_body_with_result(&path).await
}

/// Executes a single retention policy by id.
pub async fn execute_policy(
    token: &str,
    policy_id: &str,
    dry_run: bool,
) -> Result<RetentionExecutionResult, ApiError> {
    let path = format!("/retention/policies/{}/execute?dry_run={}", policy_id, dry_run);
    client(token).post_no_body_with_result(&path).await
}
