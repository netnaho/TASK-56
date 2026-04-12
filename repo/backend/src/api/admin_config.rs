//! Admin configuration routes — key/value store backed by `admin_settings`.
//!
//! Every endpoint in this module is behind `AdminOnly`. Reads are audited
//! with `ADMIN_CONFIG_READ`; writes are audited with `ADMIN_CONFIG_WRITE`
//! and carry the old & new values in the payload so diffs are reviewable.

use rocket::serde::json::Json;
use rocket::State;
use serde::{Deserialize, Serialize};
use sqlx::{MySqlPool, Row};

use crate::api::guards::AdminOnly;
use crate::application::audit_service::{self, actions, AuditEvent};
use crate::errors::{AppError, AppResult};

pub fn routes() -> Vec<rocket::Route> {
    routes![list_settings, get_setting, update_setting]
}

#[derive(Debug, Serialize)]
pub struct AdminSetting {
    pub key: String,
    pub value: serde_json::Value,
    pub description: Option<String>,
    pub updated_at: chrono::NaiveDateTime,
}

#[derive(Debug, Deserialize)]
pub struct UpdateSettingInput {
    pub value: serde_json::Value,
    pub description: Option<String>,
}

/// GET /api/v1/admin/config — list every setting.
#[get("/")]
pub async fn list_settings(
    admin: AdminOnly,
    pool: &State<MySqlPool>,
) -> AppResult<Json<Vec<AdminSetting>>> {
    let admin = admin.into_inner();
    let rows = sqlx::query(
        r#"
        SELECT setting_key,
               CAST(setting_value AS CHAR) AS value_text,
               description, updated_at
          FROM admin_settings
         ORDER BY setting_key ASC
        "#,
    )
    .fetch_all(pool.inner())
    .await
    .map_err(|e| AppError::Database(format!("admin list: {}", e)))?;

    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let key: String = row
            .try_get("setting_key")
            .map_err(|e| AppError::Database(e.to_string()))?;
        let value_text: String = row
            .try_get("value_text")
            .map_err(|e| AppError::Database(e.to_string()))?;
        let description: Option<String> = row
            .try_get("description")
            .map_err(|e| AppError::Database(e.to_string()))?;
        let updated_at: chrono::NaiveDateTime = row
            .try_get("updated_at")
            .map_err(|e| AppError::Database(e.to_string()))?;
        let value: serde_json::Value =
            serde_json::from_str(&value_text).unwrap_or(serde_json::Value::Null);
        out.push(AdminSetting {
            key,
            value,
            description,
            updated_at,
        });
    }

    let _ = audit_service::record(
        pool.inner(),
        AuditEvent {
            actor_id: Some(admin.user_id),
            actor_email: Some(&admin.email),
            action: actions::ADMIN_CONFIG_READ,
            target_entity_type: Some("admin_settings"),
            target_entity_id: None,
            change_payload: Some(serde_json::json!({ "count": out.len() })),
            ip_address: None,
            user_agent: None,
        },
    )
    .await;

    Ok(Json(out))
}

/// GET /api/v1/admin/config/<key> — fetch a single setting.
#[get("/<key>")]
pub async fn get_setting(
    _admin: AdminOnly,
    pool: &State<MySqlPool>,
    key: &str,
) -> AppResult<Json<AdminSetting>> {
    let row = sqlx::query(
        r#"
        SELECT setting_key,
               CAST(setting_value AS CHAR) AS value_text,
               description, updated_at
          FROM admin_settings
         WHERE setting_key = ?
         LIMIT 1
        "#,
    )
    .bind(key)
    .fetch_optional(pool.inner())
    .await
    .map_err(|e| AppError::Database(format!("admin get: {}", e)))?
    .ok_or_else(|| AppError::NotFound(format!("admin setting '{}'", key)))?;

    let value_text: String = row
        .try_get("value_text")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let description: Option<String> = row
        .try_get("description")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let updated_at: chrono::NaiveDateTime = row
        .try_get("updated_at")
        .map_err(|e| AppError::Database(e.to_string()))?;
    Ok(Json(AdminSetting {
        key: key.to_string(),
        value: serde_json::from_str(&value_text).unwrap_or(serde_json::Value::Null),
        description,
        updated_at,
    }))
}

/// PUT /api/v1/admin/config/<key> — upsert a setting.
///
/// The payload is stored as-is (the column type is JSON). Writes are
/// audited with the old value for reviewability.
#[put("/<key>", data = "<body>")]
pub async fn update_setting(
    admin: AdminOnly,
    pool: &State<MySqlPool>,
    key: &str,
    body: Json<UpdateSettingInput>,
) -> AppResult<Json<AdminSetting>> {
    let admin = admin.into_inner();
    let input = body.into_inner();

    // Fetch the old value so the audit payload shows the diff.
    let old_row = sqlx::query(
        r#"SELECT CAST(setting_value AS CHAR) AS value_text FROM admin_settings WHERE setting_key = ?"#,
    )
    .bind(key)
    .fetch_optional(pool.inner())
    .await
    .map_err(|e| AppError::Database(format!("admin update pre-read: {}", e)))?;
    let old_value: Option<serde_json::Value> = old_row.and_then(|r| {
        r.try_get::<String, _>("value_text")
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
    });

    let new_value_text = serde_json::to_string(&input.value)
        .map_err(|e| AppError::Validation(format!("value is not valid JSON: {}", e)))?;

    sqlx::query(
        r#"
        INSERT INTO admin_settings (setting_key, setting_value, description, updated_by)
        VALUES (?, CAST(? AS JSON), ?, ?)
        ON DUPLICATE KEY UPDATE
            setting_value = VALUES(setting_value),
            description   = COALESCE(VALUES(description), description),
            updated_by    = VALUES(updated_by)
        "#,
    )
    .bind(key)
    .bind(&new_value_text)
    .bind(input.description.as_deref())
    .bind(admin.user_id.to_string())
    .execute(pool.inner())
    .await
    .map_err(|e| AppError::Database(format!("admin upsert: {}", e)))?;

    audit_service::record(
        pool.inner(),
        AuditEvent {
            actor_id: Some(admin.user_id),
            actor_email: Some(&admin.email),
            action: actions::ADMIN_CONFIG_WRITE,
            target_entity_type: Some("admin_settings"),
            target_entity_id: None,
            change_payload: Some(serde_json::json!({
                "key": key,
                "old_value": old_value,
                "new_value": input.value,
            })),
            ip_address: None,
            user_agent: None,
        },
    )
    .await?;

    // Return the freshly stored setting.
    get_setting(AdminOnly(admin), pool, key).await
}
