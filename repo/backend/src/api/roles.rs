//! Role management routes.
//!
//! Roles are seeded at startup and correspond 1-to-1 with the `Role` enum
//! in `application::principal`. The five roles (admin, librarian,
//! department_head, instructor, viewer) are fixed by the RBAC capability
//! matrix; adding or removing roles requires code changes.
//!
//! This module exposes read-only endpoints. Role mutation endpoints are
//! intentionally absent — the capability matrix is defined in code
//! (`application::authorization::role_allows`) and cannot be changed via API.

use rocket::serde::json::Json;
use rocket::State;
use serde::Serialize;
use sqlx::{MySqlPool, Row};

use crate::api::guards::AdminOnly;
use crate::errors::{AppError, AppResult};

pub fn routes() -> Vec<rocket::Route> {
    routes![list_roles, get_role]
}

/// A role as stored in the database.
#[derive(Debug, Serialize)]
pub struct RoleView {
    pub id: String,
    pub name: String,
    pub display_name: String,
    pub description: Option<String>,
    /// Permission key names granted to this role (from `role_permissions`
    /// joined to `permissions`).
    pub permissions: Vec<String>,
}

/// GET /api/v1/roles — list all roles with their permissions.
///
/// Admin-only. Useful for admin UIs that need to display the full role list.
#[get("/")]
pub async fn list_roles(
    _admin: AdminOnly,
    pool: &State<MySqlPool>,
) -> AppResult<Json<Vec<RoleView>>> {
    let role_rows = sqlx::query(
        r#"SELECT id, name, display_name, description
             FROM roles
            ORDER BY name ASC"#,
    )
    .fetch_all(pool.inner())
    .await
    .map_err(|e| AppError::Database(format!("list_roles: {}", e)))?;

    let mut out = Vec::with_capacity(role_rows.len());
    for row in role_rows {
        let id: String = row.try_get("id").map_err(|e| AppError::Database(e.to_string()))?;
        let name: String = row.try_get("name").map_err(|e| AppError::Database(e.to_string()))?;
        let display_name: String = row
            .try_get("display_name")
            .map_err(|e| AppError::Database(e.to_string()))?;
        let description: Option<String> = row
            .try_get("description")
            .map_err(|e| AppError::Database(e.to_string()))?;

        let perm_rows = sqlx::query(
            r#"SELECT p.key_name
                 FROM role_permissions rp
                 JOIN permissions p ON p.id = rp.permission_id
                WHERE rp.role_id = ?
                ORDER BY p.key_name ASC"#,
        )
        .bind(&id)
        .fetch_all(pool.inner())
        .await
        .map_err(|e| AppError::Database(format!("role permissions: {}", e)))?;

        let permissions: Vec<String> = perm_rows
            .iter()
            .filter_map(|r| r.try_get::<String, _>("key_name").ok())
            .collect();

        out.push(RoleView {
            id,
            name,
            display_name,
            description,
            permissions,
        });
    }

    Ok(Json(out))
}

/// GET /api/v1/roles/<id> — fetch a single role by its UUID.
///
/// Admin-only.
#[get("/<id>")]
pub async fn get_role(
    _admin: AdminOnly,
    pool: &State<MySqlPool>,
    id: &str,
) -> AppResult<Json<RoleView>> {
    let row = sqlx::query(
        r#"SELECT id, name, display_name, description
             FROM roles
            WHERE id = ?
            LIMIT 1"#,
    )
    .bind(id)
    .fetch_optional(pool.inner())
    .await
    .map_err(|e| AppError::Database(format!("get_role: {}", e)))?
    .ok_or_else(|| AppError::NotFound(format!("role '{}'", id)))?;

    let role_id: String = row.try_get("id").map_err(|e| AppError::Database(e.to_string()))?;
    let name: String = row.try_get("name").map_err(|e| AppError::Database(e.to_string()))?;
    let display_name: String = row
        .try_get("display_name")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let description: Option<String> = row
        .try_get("description")
        .map_err(|e| AppError::Database(e.to_string()))?;

    let perm_rows = sqlx::query(
        r#"SELECT p.key_name
             FROM role_permissions rp
             JOIN permissions p ON p.id = rp.permission_id
            WHERE rp.role_id = ?
            ORDER BY p.key_name ASC"#,
    )
    .bind(&role_id)
    .fetch_all(pool.inner())
    .await
    .map_err(|e| AppError::Database(format!("role permissions: {}", e)))?;

    let permissions: Vec<String> = perm_rows
        .iter()
        .filter_map(|r| r.try_get::<String, _>("key_name").ok())
        .collect();

    Ok(Json(RoleView {
        id: role_id,
        name,
        display_name,
        description,
        permissions,
    }))
}
