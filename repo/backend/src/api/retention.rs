//! Retention policy REST routes — Phase 6 implementation.
//!
//! All routes require an authenticated principal with the
//! `RetentionManage` capability (Admin-only in the current RBAC matrix).
//!
//! # Route map
//!
//! ```text
//! GET    /api/v1/admin/retention/            → list_policies
//! POST   /api/v1/admin/retention/            → create_policy
//! GET    /api/v1/admin/retention/<id>        → get_policy
//! PUT    /api/v1/admin/retention/<id>        → update_policy
//! POST   /api/v1/admin/retention/execute     → execute_all
//! POST   /api/v1/admin/retention/<id>/execute → execute_policy
//! ```

use rocket::serde::json::Json;
use rocket::State;
use sqlx::MySqlPool;
use uuid::Uuid;

use crate::api::guards::AuthedPrincipal;
use crate::application::encryption::FieldEncryption;
use crate::application::retention_service::{
    self, CreateRetentionPolicyInput, RetentionPolicyView, UpdateRetentionPolicyInput,
};
use crate::config::AppConfig;
use crate::domain::retention::{RetentionExecutionResult, RetentionExecutionSummary};
use crate::errors::{AppError, AppResult};
use serde::Deserialize;

pub fn routes() -> Vec<rocket::Route> {
    routes![
        list_policies,
        create_policy,
        get_policy,
        update_policy,
        execute_all,
        execute_policy_by_id,
    ]
}

// ---------------------------------------------------------------------------
// Helper
// ---------------------------------------------------------------------------

fn parse_uuid(s: &str, field: &str) -> AppResult<Uuid> {
    Uuid::parse_str(s)
        .map_err(|_| AppError::Validation(format!("{} must be a valid UUID", field)))
}

// ---------------------------------------------------------------------------
// Request bodies
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct ExecuteRequest {
    /// When `true` the engine counts eligible rows but does not mutate
    /// anything. Defaults to `false`.
    #[serde(default)]
    pub dry_run: bool,
    /// When `true`, retention for `report_runs` is blocked if any unresolved
    /// actionable legacy artifacts (artifact_dek IS NULL, not permanently
    /// terminal) exist in the expiry window.  Returns HTTP 409 with error
    /// code `strict_mode_blocked` and a remediation hint.
    ///
    /// Defaults to `false` for backward compatibility.  Set to `true` after
    /// running `POST /api/v1/admin/artifact-backfill` to completion to
    /// enforce cryptographic-erasure guarantees for all expired artifacts.
    #[serde(default)]
    pub strict_mode: bool,
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// `GET /` — list all retention policies with live eligible-row counts.
#[get("/")]
pub async fn list_policies(
    principal: AuthedPrincipal,
    pool: &State<MySqlPool>,
) -> AppResult<Json<Vec<RetentionPolicyView>>> {
    let p = principal.into_inner();
    Ok(Json(
        retention_service::list_policies(pool.inner(), &p).await?,
    ))
}

/// `POST /` — create a new retention policy.
#[post("/", format = "json", data = "<body>")]
pub async fn create_policy(
    principal: AuthedPrincipal,
    pool: &State<MySqlPool>,
    body: Json<CreateRetentionPolicyInput>,
) -> AppResult<Json<RetentionPolicyView>> {
    let p = principal.into_inner();
    Ok(Json(
        retention_service::create_policy(pool.inner(), &p, body.into_inner()).await?,
    ))
}

/// `GET /<id>` — fetch a single retention policy.
#[get("/<id>")]
pub async fn get_policy(
    principal: AuthedPrincipal,
    pool: &State<MySqlPool>,
    id: &str,
) -> AppResult<Json<RetentionPolicyView>> {
    let p = principal.into_inner();
    let uid = parse_uuid(id, "id")?;
    Ok(Json(
        retention_service::get_policy(pool.inner(), &p, uid).await?,
    ))
}

/// `PUT /<id>` — update mutable fields of a retention policy.
#[put("/<id>", format = "json", data = "<body>")]
pub async fn update_policy(
    principal: AuthedPrincipal,
    pool: &State<MySqlPool>,
    id: &str,
    body: Json<UpdateRetentionPolicyInput>,
) -> AppResult<Json<RetentionPolicyView>> {
    let p = principal.into_inner();
    let uid = parse_uuid(id, "id")?;
    Ok(Json(
        retention_service::update_policy(pool.inner(), &p, uid, body.into_inner()).await?,
    ))
}

/// `POST /execute` — run all active retention policies.
///
/// Body: `{ "dry_run": bool, "strict_mode": bool }` — both default to `false`.
///
/// When `strict_mode = true` and unresolved legacy artifacts exist, returns
/// HTTP 409 with error code `strict_mode_blocked` and a remediation hint.
#[post("/execute", format = "json", data = "<body>")]
pub async fn execute_all(
    principal: AuthedPrincipal,
    pool: &State<MySqlPool>,
    config: &State<AppConfig>,
    enc: &State<FieldEncryption>,
    body: Json<ExecuteRequest>,
) -> AppResult<Json<RetentionExecutionSummary>> {
    let p = principal.into_inner();
    Ok(Json(
        retention_service::execute_all(
            pool.inner(),
            &p,
            &config.reports_storage_path,
            body.dry_run,
            body.strict_mode,
            enc.inner(),
        )
        .await?,
    ))
}

/// `POST /<id>/execute` — run a single retention policy.
///
/// Body: `{ "dry_run": bool, "strict_mode": bool }` — both default to `false`.
///
/// When `strict_mode = true` and unresolved legacy artifacts exist, returns
/// HTTP 409 with error code `strict_mode_blocked` and a remediation hint.
#[post("/<id>/execute", format = "json", data = "<body>")]
pub async fn execute_policy_by_id(
    principal: AuthedPrincipal,
    pool: &State<MySqlPool>,
    config: &State<AppConfig>,
    enc: &State<FieldEncryption>,
    id: &str,
    body: Json<ExecuteRequest>,
) -> AppResult<Json<RetentionExecutionResult>> {
    let p = principal.into_inner();
    let uid = parse_uuid(id, "id")?;
    Ok(Json(
        retention_service::execute_policy(
            pool.inner(),
            &p,
            uid,
            &config.reports_storage_path,
            body.dry_run,
            body.strict_mode,
            enc.inner(),
        )
        .await?,
    ))
}
