//! User management routes.
//!
//! All mutating endpoints are `AdminOnly`.  Password hashing uses Argon2id
//! via `application::password`; deletion is a soft-delete (sets status to
//! `deactivated`) to preserve audit log integrity.

use rocket::serde::json::Json;
use rocket::State;
use serde::{Deserialize, Serialize};
use sqlx::{MySqlPool, Row};
use uuid::Uuid;

use crate::api::guards::{AdminOnly, AuthedPrincipal};
use crate::application::audit_service::{self, actions, AuditEvent};
use crate::application::auth_service;
use crate::application::password;
use crate::errors::{AppError, AppResult};

pub fn routes() -> Vec<rocket::Route> {
    routes![me, list_users, get_user, create_user, update_user, deactivate_user]
}

// ---------------------------------------------------------------------------
// View types
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct UserProfile {
    pub id: Uuid,
    pub email: String,
    pub display_name: String,
    pub status: String,
    pub roles: Vec<String>,
    pub department_id: Option<Uuid>,
}

// ---------------------------------------------------------------------------
// Input types
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct CreateUserInput {
    pub email: String,
    pub display_name: String,
    pub password: String,
    pub department_id: Option<String>,
    /// Role names (snake_case, e.g. "viewer", "instructor"). Defaults to
    /// ["viewer"] when absent.
    #[serde(default)]
    pub roles: Vec<String>,
}

#[derive(Deserialize)]
pub struct UpdateUserInput {
    pub display_name: Option<String>,
    /// Status must be one of: active, suspended, deactivated.
    pub status: Option<String>,
    pub department_id: Option<String>,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

async fn load_profile(pool: &MySqlPool, uid: Uuid) -> AppResult<UserProfile> {
    let row = sqlx::query(
        r#"SELECT id, email, display_name, status, department_id
             FROM users
            WHERE id = ?
            LIMIT 1"#,
    )
    .bind(uid.to_string())
    .fetch_optional(pool)
    .await
    .map_err(|e| AppError::Database(format!("load_profile: {}", e)))?
    .ok_or_else(|| AppError::NotFound("user".into()))?;

    let email: String = row.try_get("email").map_err(|e| AppError::Database(e.to_string()))?;
    let display_name: String = row
        .try_get("display_name")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let status: String = row.try_get("status").map_err(|e| AppError::Database(e.to_string()))?;
    let dept: Option<String> = row
        .try_get("department_id")
        .map_err(|e| AppError::Database(e.to_string()))?;

    let roles = auth_service::load_user_roles(pool, uid).await?;
    Ok(UserProfile {
        id: uid,
        email,
        display_name,
        status,
        roles: roles.iter().map(|r| r.as_db_name().to_string()).collect(),
        department_id: dept.as_deref().and_then(|s| Uuid::parse_str(s).ok()),
    })
}

// ---------------------------------------------------------------------------
// Endpoints
// ---------------------------------------------------------------------------

/// GET /api/v1/users/me — return the caller's own profile.
#[get("/me")]
pub async fn me(
    principal: AuthedPrincipal,
    pool: &State<MySqlPool>,
) -> AppResult<Json<UserProfile>> {
    let p = principal.into_inner();
    Ok(Json(load_profile(pool.inner(), p.user_id).await?))
}

/// GET /api/v1/users — list all users (admin only).
#[get("/")]
pub async fn list_users(
    _admin: AdminOnly,
    pool: &State<MySqlPool>,
) -> AppResult<Json<Vec<UserProfile>>> {
    let rows = sqlx::query(
        r#"SELECT id FROM users ORDER BY email ASC"#,
    )
    .fetch_all(pool.inner())
    .await
    .map_err(|e| AppError::Database(format!("list_users: {}", e)))?;

    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let id_s: String = row.try_get("id").map_err(|e| AppError::Database(e.to_string()))?;
        let uid = Uuid::parse_str(&id_s).map_err(|e| AppError::Database(e.to_string()))?;
        out.push(load_profile(pool.inner(), uid).await?);
    }
    Ok(Json(out))
}

/// GET /api/v1/users/<id> — fetch one user by UUID (admin only).
#[get("/<id>")]
pub async fn get_user(
    _admin: AdminOnly,
    pool: &State<MySqlPool>,
    id: &str,
) -> AppResult<Json<UserProfile>> {
    let uid = Uuid::parse_str(id)
        .map_err(|_| AppError::Validation("id must be a UUID".into()))?;
    Ok(Json(load_profile(pool.inner(), uid).await?))
}

/// POST /api/v1/users — create a new user (admin only).
///
/// Assigns the requested roles (defaults to `viewer`). Password is hashed
/// with Argon2id. The new user's status is `active`.
#[post("/", data = "<body>")]
pub async fn create_user(
    admin: AdminOnly,
    pool: &State<MySqlPool>,
    body: Json<CreateUserInput>,
) -> AppResult<Json<UserProfile>> {
    let admin = admin.into_inner();
    let input = body.into_inner();

    // Validate password policy before touching the DB.
    password::validate_password_policy(&input.password)?;

    let email = input.email.trim().to_lowercase();
    let hash = password::hash_password(&input.password)?;
    let new_id = Uuid::new_v4();

    // Verify the email is not already taken.
    let existing = sqlx::query(
        r#"SELECT id FROM users WHERE email = ? LIMIT 1"#,
    )
    .bind(&email)
    .fetch_optional(pool.inner())
    .await
    .map_err(|e| AppError::Database(format!("create_user pre-check: {}", e)))?;

    if existing.is_some() {
        return Err(AppError::Conflict(format!(
            "a user with email '{}' already exists",
            email
        )));
    }

    // Resolve department UUID if provided.
    let dept_id = match &input.department_id {
        None => None,
        Some(s) if s.is_empty() => None,
        Some(s) => Some(
            Uuid::parse_str(s)
                .map_err(|_| AppError::Validation("department_id must be a UUID".into()))?,
        ),
    };

    sqlx::query(
        r#"INSERT INTO users (id, email, display_name, password_hash, status, department_id)
           VALUES (?, ?, ?, ?, 'active', ?)"#,
    )
    .bind(new_id.to_string())
    .bind(&email)
    .bind(input.display_name.trim())
    .bind(&hash)
    .bind(dept_id.map(|u| u.to_string()))
    .execute(pool.inner())
    .await
    .map_err(|e| AppError::Database(format!("create_user insert: {}", e)))?;

    // Assign roles (default to "viewer" if none specified).
    let role_names: Vec<&str> = if input.roles.is_empty() {
        vec!["viewer"]
    } else {
        input.roles.iter().map(|s| s.as_str()).collect()
    };

    for role_name in &role_names {
        let role_row = sqlx::query(r#"SELECT id FROM roles WHERE name = ? LIMIT 1"#)
            .bind(role_name)
            .fetch_optional(pool.inner())
            .await
            .map_err(|e| AppError::Database(format!("resolve role: {}", e)))?;

        let rr = role_row.ok_or_else(|| {
            AppError::Validation(format!("unknown role '{}'", role_name))
        })?;
        let role_id: String = rr.try_get("id").map_err(|e| AppError::Database(e.to_string()))?;
        sqlx::query(
            r#"INSERT IGNORE INTO user_roles (user_id, role_id, assigned_by)
               VALUES (?, ?, ?)"#,
        )
        .bind(new_id.to_string())
        .bind(&role_id)
        .bind(admin.user_id.to_string())
        .execute(pool.inner())
        .await
        .map_err(|e| AppError::Database(format!("assign role: {}", e)))?;
    }

    audit_service::record(
        pool.inner(),
        AuditEvent {
            actor_id: Some(admin.user_id),
            actor_email: Some(&admin.email),
            action: actions::USER_CREATE,
            target_entity_type: Some("user"),
            target_entity_id: Some(new_id),
            change_payload: Some(serde_json::json!({
                "email": email,
                "roles": role_names,
            })),
            ip_address: None,
            user_agent: None,
        },
    )
    .await?;

    Ok(Json(load_profile(pool.inner(), new_id).await?))
}

/// PUT /api/v1/users/<id> — update display name, status, or department
/// (admin only).
///
/// Changing status to `deactivated` invalidates all active sessions. Changing
/// email is intentionally not supported — email is the unique login identity
/// and changes to it require a separate verification workflow.
#[put("/<id>", data = "<body>")]
pub async fn update_user(
    admin: AdminOnly,
    pool: &State<MySqlPool>,
    id: &str,
    body: Json<UpdateUserInput>,
) -> AppResult<Json<UserProfile>> {
    let admin = admin.into_inner();
    let uid =
        Uuid::parse_str(id).map_err(|_| AppError::Validation("id must be a UUID".into()))?;
    let input = body.into_inner();

    // Validate status if provided.
    if let Some(ref s) = input.status {
        match s.as_str() {
            "active" | "suspended" | "deactivated" => {}
            other => {
                return Err(AppError::Validation(format!(
                    "invalid status '{}'; must be one of: active, suspended, deactivated",
                    other
                )))
            }
        }
    }

    let dept_id = match &input.department_id {
        None => None,
        Some(s) if s.is_empty() => None,
        Some(s) => Some(
            Uuid::parse_str(s)
                .map_err(|_| AppError::Validation("department_id must be a UUID".into()))?,
        ),
    };

    // Build the update dynamically based on what was provided.
    // We always re-fetch the row first so we can audit the diff.
    let before = load_profile(pool.inner(), uid).await?;

    if let Some(ref name) = input.display_name {
        sqlx::query("UPDATE users SET display_name = ? WHERE id = ?")
            .bind(name.trim())
            .bind(uid.to_string())
            .execute(pool.inner())
            .await
            .map_err(|e| AppError::Database(format!("update display_name: {}", e)))?;
    }

    if let Some(ref status) = input.status {
        sqlx::query("UPDATE users SET status = ? WHERE id = ?")
            .bind(status.as_str())
            .bind(uid.to_string())
            .execute(pool.inner())
            .await
            .map_err(|e| AppError::Database(format!("update status: {}", e)))?;

        // Revoke all sessions when deactivating.
        if status == "deactivated" {
            sqlx::query("UPDATE sessions SET revoked_at = NOW() WHERE user_id = ? AND revoked_at IS NULL")
                .bind(uid.to_string())
                .execute(pool.inner())
                .await
                .map_err(|e| AppError::Database(format!("revoke sessions: {}", e)))?;
        }
    }

    if input.department_id.is_some() {
        sqlx::query("UPDATE users SET department_id = ? WHERE id = ?")
            .bind(dept_id.map(|u| u.to_string()))
            .bind(uid.to_string())
            .execute(pool.inner())
            .await
            .map_err(|e| AppError::Database(format!("update department: {}", e)))?;
    }

    audit_service::record(
        pool.inner(),
        AuditEvent {
            actor_id: Some(admin.user_id),
            actor_email: Some(&admin.email),
            action: actions::USER_UPDATE,
            target_entity_type: Some("user"),
            target_entity_id: Some(uid),
            change_payload: Some(serde_json::json!({
                "before": {
                    "display_name": before.display_name,
                    "status": before.status,
                    "department_id": before.department_id,
                },
                "changes": {
                    "display_name": input.display_name,
                    "status": input.status,
                    "department_id": input.department_id,
                },
            })),
            ip_address: None,
            user_agent: None,
        },
    )
    .await?;

    Ok(Json(load_profile(pool.inner(), uid).await?))
}

/// DELETE /api/v1/users/<id> — deactivate a user (admin only).
///
/// Soft-deletes the account: sets `status = 'deactivated'` and revokes all
/// active sessions. The user row is **not** deleted so that audit log
/// entries pointing to this user remain coherent.
///
/// An admin cannot deactivate themselves.
#[delete("/<id>")]
pub async fn deactivate_user(
    admin: AdminOnly,
    pool: &State<MySqlPool>,
    id: &str,
) -> AppResult<Json<UserProfile>> {
    let admin = admin.into_inner();
    let uid =
        Uuid::parse_str(id).map_err(|_| AppError::Validation("id must be a UUID".into()))?;

    if uid == admin.user_id {
        return Err(AppError::Validation(
            "cannot deactivate your own account".into(),
        ));
    }

    sqlx::query("UPDATE users SET status = 'deactivated' WHERE id = ?")
        .bind(uid.to_string())
        .execute(pool.inner())
        .await
        .map_err(|e| AppError::Database(format!("deactivate user: {}", e)))?;

    // Revoke active sessions so the token cannot be used again.
    sqlx::query("UPDATE sessions SET revoked_at = NOW() WHERE user_id = ? AND revoked_at IS NULL")
        .bind(uid.to_string())
        .execute(pool.inner())
        .await
        .map_err(|e| AppError::Database(format!("revoke sessions: {}", e)))?;

    audit_service::record(
        pool.inner(),
        AuditEvent {
            actor_id: Some(admin.user_id),
            actor_email: Some(&admin.email),
            action: actions::USER_DEACTIVATE,
            target_entity_type: Some("user"),
            target_entity_id: Some(uid),
            change_payload: Some(serde_json::json!({ "reason": "admin_deactivation" })),
            ip_address: None,
            user_agent: None,
        },
    )
    .await?;

    Ok(Json(load_profile(pool.inner(), uid).await?))
}
