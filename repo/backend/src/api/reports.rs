//! Report management API endpoints.
//!
//! All endpoints require a valid `AuthedPrincipal`. Capability enforcement
//! (ReportRead / ReportExecute / ReportManage) is delegated to
//! `report_service` so the handler layer stays thin.

use rocket::response::status::Accepted;
use rocket::serde::json::Json;
use rocket::State;
use sqlx::MySqlPool;
use uuid::Uuid;

use crate::api::download::BinaryDownload;
use crate::api::guards::AuthedPrincipal;
use crate::application::encryption::FieldEncryption;
use crate::application::report_service::{
    self, CreateReportInput, CreateScheduleInput, ReportRunView, ReportScheduleView, ReportView,
    TriggerRunInput, UpdateReportInput, UpdateScheduleInput,
};
use crate::config::AppConfig;
use crate::errors::{AppError, AppResult};

pub fn routes() -> Vec<rocket::Route> {
    routes![
        list_reports,
        create_report,
        get_report,
        update_report,
        trigger_run,
        list_runs,
        get_run,
        download_artifact,
        list_schedules,
        create_schedule,
        update_schedule,
        delete_schedule,
    ]
}

// ─── Reports ──────────────────────────────────────────────────────────────────

/// GET / — list all reports visible to the caller.
#[get("/")]
pub async fn list_reports(
    principal: AuthedPrincipal,
    pool: &State<MySqlPool>,
) -> AppResult<Json<Vec<ReportView>>> {
    let p = principal.into_inner();
    let result = report_service::list_reports(pool.inner(), &p).await?;
    Ok(Json(result))
}

/// POST / — create a new report definition.
#[post("/", format = "json", data = "<body>")]
pub async fn create_report(
    principal: AuthedPrincipal,
    pool: &State<MySqlPool>,
    body: Json<CreateReportInput>,
) -> AppResult<Json<ReportView>> {
    let p = principal.into_inner();
    let result = report_service::create_report(pool.inner(), &p, body.into_inner()).await?;
    Ok(Json(result))
}

/// GET /<id> — retrieve a single report.
#[get("/<id>")]
pub async fn get_report(
    principal: AuthedPrincipal,
    pool: &State<MySqlPool>,
    id: &str,
) -> AppResult<Json<ReportView>> {
    let p = principal.into_inner();
    let report_id = Uuid::parse_str(id)
        .map_err(|_| AppError::Validation("invalid report id".to_string()))?;
    let result = report_service::get_report(pool.inner(), &p, report_id).await?;
    Ok(Json(result))
}

/// PUT /<id> — update report metadata (title, description, default_format).
#[put("/<id>", format = "json", data = "<body>")]
pub async fn update_report(
    principal: AuthedPrincipal,
    pool: &State<MySqlPool>,
    id: &str,
    body: Json<UpdateReportInput>,
) -> AppResult<Json<ReportView>> {
    let p = principal.into_inner();
    let report_id = Uuid::parse_str(id)
        .map_err(|_| AppError::Validation("invalid report id".to_string()))?;
    let result =
        report_service::update_report(pool.inner(), &p, report_id, body.into_inner()).await?;
    Ok(Json(result))
}

// ─── Runs ─────────────────────────────────────────────────────────────────────

/// POST /<id>/run — trigger an on-demand run. Returns 202 Accepted.
#[post("/<id>/run", format = "json", data = "<body>")]
pub async fn trigger_run(
    principal: AuthedPrincipal,
    pool: &State<MySqlPool>,
    config: &State<AppConfig>,
    enc: &State<FieldEncryption>,
    id: &str,
    body: Json<TriggerRunInput>,
) -> AppResult<Accepted<Json<ReportRunView>>> {
    let p = principal.into_inner();
    let report_id = Uuid::parse_str(id)
        .map_err(|_| AppError::Validation("invalid report id".to_string()))?;
    let run_view = report_service::trigger_run(
        pool.inner(),
        &p,
        report_id,
        body.into_inner().format,
        &config.reports_storage_path,
        enc.inner(),
    )
    .await?;
    Ok(Accepted(Json(run_view)))
}

/// GET /<id>/runs — list previous runs for a report.
#[get("/<id>/runs", rank = 2)]
pub async fn list_runs(
    principal: AuthedPrincipal,
    pool: &State<MySqlPool>,
    config: &State<AppConfig>,
    id: &str,
) -> AppResult<Json<Vec<ReportRunView>>> {
    let p = principal.into_inner();
    let report_id = Uuid::parse_str(id)
        .map_err(|_| AppError::Validation("invalid report id".to_string()))?;
    let result =
        report_service::list_runs(pool.inner(), &p, report_id, &config.reports_storage_path)
            .await?;
    Ok(Json(result))
}

/// GET /runs/<run_id> — get a single run by its UUID.
#[get("/runs/<run_id>")]
pub async fn get_run(
    principal: AuthedPrincipal,
    pool: &State<MySqlPool>,
    config: &State<AppConfig>,
    run_id: &str,
) -> AppResult<Json<ReportRunView>> {
    let p = principal.into_inner();
    let run_uuid = Uuid::parse_str(run_id)
        .map_err(|_| AppError::Validation("invalid run_id".to_string()))?;
    let result =
        report_service::get_run(pool.inner(), &p, run_uuid, &config.reports_storage_path).await?;
    Ok(Json(result))
}

/// GET /runs/<run_id>/download — download the artifact file.
#[get("/runs/<run_id>/download")]
pub async fn download_artifact(
    principal: AuthedPrincipal,
    pool: &State<MySqlPool>,
    config: &State<AppConfig>,
    enc: &State<FieldEncryption>,
    run_id: &str,
) -> AppResult<BinaryDownload> {
    let p = principal.into_inner();
    let run_uuid = Uuid::parse_str(run_id)
        .map_err(|_| AppError::Validation("invalid run_id".to_string()))?;
    let (bytes, mime, filename) =
        report_service::download_artifact(pool.inner(), &p, run_uuid, &config.reports_storage_path, enc.inner())
            .await?;
    Ok(BinaryDownload {
        bytes,
        mime,
        filename,
    })
}

// ─── Schedules ────────────────────────────────────────────────────────────────

/// GET /<id>/schedules — list schedules for a report.
#[get("/<id>/schedules", rank = 2)]
pub async fn list_schedules(
    principal: AuthedPrincipal,
    pool: &State<MySqlPool>,
    id: &str,
) -> AppResult<Json<Vec<ReportScheduleView>>> {
    let p = principal.into_inner();
    let report_id = Uuid::parse_str(id)
        .map_err(|_| AppError::Validation("invalid report id".to_string()))?;
    let result = report_service::list_schedules(pool.inner(), &p, report_id).await?;
    Ok(Json(result))
}

/// POST /<id>/schedules — create a new schedule for a report.
#[post("/<id>/schedules", format = "json", data = "<body>")]
pub async fn create_schedule(
    principal: AuthedPrincipal,
    pool: &State<MySqlPool>,
    id: &str,
    body: Json<CreateScheduleInput>,
) -> AppResult<Json<ReportScheduleView>> {
    let p = principal.into_inner();
    let report_id = Uuid::parse_str(id)
        .map_err(|_| AppError::Validation("invalid report id".to_string()))?;
    let result =
        report_service::create_schedule(pool.inner(), &p, report_id, body.into_inner()).await?;
    Ok(Json(result))
}

/// PUT /schedules/<schedule_id> — update an existing schedule.
#[put("/schedules/<schedule_id>", format = "json", data = "<body>")]
pub async fn update_schedule(
    principal: AuthedPrincipal,
    pool: &State<MySqlPool>,
    schedule_id: &str,
    body: Json<UpdateScheduleInput>,
) -> AppResult<Json<ReportScheduleView>> {
    let p = principal.into_inner();
    let sched_uuid = Uuid::parse_str(schedule_id)
        .map_err(|_| AppError::Validation("invalid schedule_id".to_string()))?;
    let result =
        report_service::update_schedule(pool.inner(), &p, sched_uuid, body.into_inner()).await?;
    Ok(Json(result))
}

/// DELETE /schedules/<schedule_id> — delete a schedule.
#[delete("/schedules/<schedule_id>")]
pub async fn delete_schedule(
    principal: AuthedPrincipal,
    pool: &State<MySqlPool>,
    schedule_id: &str,
) -> AppResult<()> {
    let p = principal.into_inner();
    let sched_uuid = Uuid::parse_str(schedule_id)
        .map_err(|_| AppError::Validation("invalid schedule_id".to_string()))?;
    report_service::delete_schedule(pool.inner(), &p, sched_uuid).await?;
    Ok(())
}
