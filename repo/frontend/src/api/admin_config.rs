//! Typed wrappers for the admin configuration endpoints.
//!
//! All endpoints require an admin bearer token.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::api::client::{ApiClient, ApiError};

/// A single admin configuration setting.
#[derive(Debug, Clone, Deserialize)]
pub struct AdminSetting {
    pub key: String,
    pub value: Value,
    pub description: Option<String>,
    pub updated_at: String,
}

/// Body for updating a setting.
#[derive(Debug, Serialize)]
pub struct UpdateSettingInput {
    pub value: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

fn client(token: &str) -> ApiClient {
    ApiClient::new(Some(token.to_string()))
}

/// GET /api/v1/admin/config — fetch all settings.
pub async fn list_settings(token: &str) -> Result<Vec<AdminSetting>, ApiError> {
    client(token).get_json("/admin/config").await
}

/// PUT /api/v1/admin/config/<key> — upsert a setting.
pub async fn update_setting(
    token: &str,
    key: &str,
    input: &UpdateSettingInput,
) -> Result<AdminSetting, ApiError> {
    let path = format!("/admin/config/{}", key);
    client(token).put_json(&path, input).await
}
