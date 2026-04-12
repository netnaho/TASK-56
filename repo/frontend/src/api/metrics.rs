//! Typed wrappers for the Phase 5 metric-definition endpoints.
//!
//! Metric definitions follow the same versioned draft → approved →
//! published flow as journals and sections, so this module mirrors the
//! shape of [`crate::api::journals`].

use serde::{Deserialize, Serialize};

use crate::api::client::{ApiClient, ApiError};

/// Lineage reference linking a metric version to an upstream metric's
/// specific version. Rendered as a chip in the UI.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct LineageRef {
    pub definition_id: String,
    pub version_id: String,
}

/// A single metric version as delivered by the backend.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct MetricVersionView {
    pub id: String,
    pub metric_definition_id: String,
    pub version_number: i32,
    pub formula: String,
    #[serde(default)]
    pub description: Option<String>,
    pub metric_type: String,
    #[serde(default)]
    pub window_seconds: Option<i64>,
    #[serde(default)]
    pub lineage_refs: Vec<LineageRef>,
    #[serde(default)]
    pub change_summary: Option<String>,
    pub state: String,
    #[serde(default)]
    pub created_by: Option<String>,
    pub created_at: String,
    #[serde(default)]
    pub approved_at: Option<String>,
    #[serde(default)]
    pub published_at: Option<String>,
}

/// A metric definition with its effective version pre-hydrated.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct MetricDefinitionView {
    pub id: String,
    pub key_name: String,
    pub display_name: String,
    #[serde(default)]
    pub unit: Option<String>,
    pub polarity: String,
    #[serde(default)]
    pub current_version_id: Option<String>,
    #[serde(default)]
    pub latest_version_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    #[serde(default)]
    pub effective_version: Option<MetricVersionView>,
}

/// Body accepted by `POST /metrics`.
#[derive(Debug, Clone, Serialize)]
pub struct MetricCreateInput {
    pub key_name: String,
    pub display_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
    pub polarity: String,
    pub formula: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub metric_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub window_seconds: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub change_summary: Option<String>,
    pub lineage_refs: Vec<LineageRef>,
}

/// Body accepted by `PUT /metrics/<id>`.
#[derive(Debug, Clone, Serialize)]
pub struct MetricEditInput {
    pub formula: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub metric_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub window_seconds: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub change_summary: Option<String>,
    pub lineage_refs: Vec<LineageRef>,
}

fn client(token: &str) -> ApiClient {
    ApiClient::new(Some(token.to_string()))
}

/// Fetches every metric definition.
pub async fn list(token: &str) -> Result<Vec<MetricDefinitionView>, ApiError> {
    client(token).get_json("/metrics").await
}

/// Fetches a single metric definition by id (with effective version).
pub async fn get(token: &str, id: &str) -> Result<MetricDefinitionView, ApiError> {
    let path = format!("/metrics/{}", id);
    client(token).get_json(&path).await
}

/// Creates a new metric definition and returns the hydrated view.
pub async fn create(
    token: &str,
    input: &MetricCreateInput,
) -> Result<MetricDefinitionView, ApiError> {
    client(token).post_json("/metrics", input).await
}

/// Edits a metric by creating a new draft version.
pub async fn edit(
    token: &str,
    id: &str,
    input: &MetricEditInput,
) -> Result<MetricVersionView, ApiError> {
    let path = format!("/metrics/{}", id);
    client(token).put_json(&path, input).await
}

/// Lists every version for a metric (most recent first).
pub async fn list_versions(
    token: &str,
    id: &str,
) -> Result<Vec<MetricVersionView>, ApiError> {
    let path = format!("/metrics/{}/versions", id);
    client(token).get_json(&path).await
}

/// Transitions a draft version to `approved`.
pub async fn approve(
    token: &str,
    id: &str,
    version_id: &str,
) -> Result<MetricVersionView, ApiError> {
    let path = format!("/metrics/{}/versions/{}/approve", id, version_id);
    client(token).post_no_body_with_result(&path).await
}

/// Publishes an approved version, returning the updated metric view.
/// Admin-only per backend enforcement.
pub async fn publish(
    token: &str,
    id: &str,
    version_id: &str,
) -> Result<MetricDefinitionView, ApiError> {
    let path = format!("/metrics/{}/versions/{}/publish", id, version_id);
    client(token).post_no_body_with_result(&path).await
}

/// Verifies a dashboard widget bound to a published metric version.
pub async fn verify_widget(token: &str, widget_id: &str) -> Result<(), ApiError> {
    #[derive(Debug, Deserialize)]
    struct VerifyOk {
        #[allow(dead_code)]
        ok: bool,
    }
    let path = format!("/metrics/widgets/{}/verify", widget_id);
    let _: VerifyOk = client(token).post_no_body_with_result(&path).await?;
    Ok(())
}
