//! Course catalog REST routes — Phase 4 implementation.
//!
//! Routes under `/api/v1/courses`:
//!
//! * CRUD: list, create, get, new-draft, list_versions, get_version,
//!   approve, publish
//! * Prerequisites: list, add, remove
//! * Templates: `/template.csv`, `/template.xlsx`
//! * Export:    `/export.csv`,   `/export.xlsx`   (department-scoped)
//! * Import:    `POST /import` (multipart: `file`, `mode=dry_run|commit`)

use rocket::form::Form;
use rocket::fs::TempFile;
use rocket::serde::json::Json;
use rocket::State;
use serde::Deserialize;
use sqlx::MySqlPool;
use uuid::Uuid;

use crate::api::download::BinaryDownload;
use crate::api::guards::AuthedPrincipal;
use crate::application::course_service::{
    self, CourseCreateInput, CourseEditInput, CourseVersionView, CourseView, PrerequisiteRef,
};
use crate::application::export_service;
use crate::application::import_service::{
    self, ImportFormat, ImportMode, ImportReport,
};
use crate::config::AppConfig;
use crate::errors::{AppError, AppResult};

pub fn routes() -> Vec<rocket::Route> {
    routes![
        list_courses,
        create_course,
        get_course,
        new_draft,
        list_versions,
        get_version,
        approve_version,
        publish_version,
        list_prerequisites,
        add_prerequisite,
        remove_prerequisite,
        template_csv,
        template_xlsx,
        export_csv,
        export_xlsx,
        import_courses,
    ]
}

fn parse_uuid(s: &str, field: &str) -> AppResult<Uuid> {
    Uuid::parse_str(s).map_err(|_| AppError::Validation(format!("{} must be a UUID", field)))
}

// ── CRUD + versioning ───────────────────────────────────────────────────────

#[get("/?<department_id>&<limit>&<offset>")]
pub async fn list_courses(
    principal: AuthedPrincipal,
    pool: &State<MySqlPool>,
    department_id: Option<String>,
    limit: Option<u32>,
    offset: Option<u32>,
) -> AppResult<Json<Vec<CourseView>>> {
    let p = principal.into_inner();
    let dept = match department_id {
        Some(ref s) if !s.is_empty() => Some(parse_uuid(s, "department_id")?),
        _ => None,
    };
    let views = course_service::list_courses(
        pool.inner(),
        &p,
        dept,
        limit.unwrap_or(100),
        offset.unwrap_or(0),
    )
    .await?;
    Ok(Json(views))
}

#[post("/", format = "json", data = "<body>")]
pub async fn create_course(
    principal: AuthedPrincipal,
    pool: &State<MySqlPool>,
    body: Json<CourseCreateInput>,
) -> AppResult<Json<CourseView>> {
    let p = principal.into_inner();
    let view = course_service::create_course(pool.inner(), &p, body.into_inner()).await?;
    Ok(Json(view))
}

#[get("/<id>", rank = 2)]
pub async fn get_course(
    principal: AuthedPrincipal,
    pool: &State<MySqlPool>,
    id: &str,
) -> AppResult<Json<CourseView>> {
    let p = principal.into_inner();
    let cid = parse_uuid(id, "id")?;
    let view = course_service::get_course_by_id(pool.inner(), &p, cid).await?;
    Ok(Json(view))
}

#[put("/<id>", format = "json", data = "<body>")]
pub async fn new_draft(
    principal: AuthedPrincipal,
    pool: &State<MySqlPool>,
    id: &str,
    body: Json<CourseEditInput>,
) -> AppResult<Json<CourseVersionView>> {
    let p = principal.into_inner();
    let cid = parse_uuid(id, "id")?;
    let v = course_service::create_draft_version(pool.inner(), &p, cid, body.into_inner()).await?;
    Ok(Json(v))
}

#[get("/<id>/versions")]
pub async fn list_versions(
    principal: AuthedPrincipal,
    pool: &State<MySqlPool>,
    id: &str,
) -> AppResult<Json<Vec<CourseVersionView>>> {
    let p = principal.into_inner();
    let cid = parse_uuid(id, "id")?;
    Ok(Json(course_service::list_versions(pool.inner(), &p, cid).await?))
}

#[get("/<id>/versions/<vid>")]
pub async fn get_version(
    principal: AuthedPrincipal,
    pool: &State<MySqlPool>,
    id: &str,
    vid: &str,
) -> AppResult<Json<CourseVersionView>> {
    // Validate UUID shape; the service itself loads the version and
    // enforces the parent match.
    let _ = parse_uuid(id, "id")?;
    let _ = parse_uuid(vid, "vid")?;
    // Phase 4 exposes the read via course_service::list_versions for
    // editors; single-version read goes through load_version which is
    // pub(crate) — re-expose via a thin wrapper below.
    let p = principal.into_inner();
    let cid = parse_uuid(id, "id")?;
    let v = parse_uuid(vid, "vid")?;
    // list_versions already filters to the caller; find the one we want.
    let versions = course_service::list_versions(pool.inner(), &p, cid).await?;
    let found = versions
        .into_iter()
        .find(|x| x.id == v)
        .ok_or_else(|| AppError::NotFound(format!("course_version {}", vid)))?;
    Ok(Json(found))
}

#[post("/<id>/versions/<vid>/approve")]
pub async fn approve_version(
    principal: AuthedPrincipal,
    pool: &State<MySqlPool>,
    id: &str,
    vid: &str,
) -> AppResult<Json<CourseVersionView>> {
    let p = principal.into_inner();
    let cid = parse_uuid(id, "id")?;
    let v = parse_uuid(vid, "vid")?;
    Ok(Json(
        course_service::approve_version(pool.inner(), &p, cid, v).await?,
    ))
}

#[post("/<id>/versions/<vid>/publish")]
pub async fn publish_version(
    principal: AuthedPrincipal,
    pool: &State<MySqlPool>,
    id: &str,
    vid: &str,
) -> AppResult<Json<CourseView>> {
    let p = principal.into_inner();
    let cid = parse_uuid(id, "id")?;
    let v = parse_uuid(vid, "vid")?;
    Ok(Json(
        course_service::publish_version(pool.inner(), &p, cid, v).await?,
    ))
}

// ── Prerequisites ───────────────────────────────────────────────────────────

#[get("/<id>/prerequisites")]
pub async fn list_prerequisites(
    principal: AuthedPrincipal,
    pool: &State<MySqlPool>,
    id: &str,
) -> AppResult<Json<Vec<PrerequisiteRef>>> {
    let p = principal.into_inner();
    let cid = parse_uuid(id, "id")?;
    // Returning the prereqs of a CourseView — reusing get_course_by_id so
    // authorization lives in the service.
    let view = course_service::get_course_by_id(pool.inner(), &p, cid).await?;
    Ok(Json(view.prerequisites))
}

#[derive(Deserialize)]
pub struct PrereqAddInput {
    pub prerequisite_course_id: Uuid,
    pub min_grade: Option<String>,
}

#[post("/<id>/prerequisites", format = "json", data = "<body>")]
pub async fn add_prerequisite(
    principal: AuthedPrincipal,
    pool: &State<MySqlPool>,
    id: &str,
    body: Json<PrereqAddInput>,
) -> AppResult<Json<serde_json::Value>> {
    let p = principal.into_inner();
    let cid = parse_uuid(id, "id")?;
    let input = body.into_inner();
    course_service::add_prerequisite(
        pool.inner(),
        &p,
        cid,
        input.prerequisite_course_id,
        input.min_grade,
    )
    .await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

#[delete("/<id>/prerequisites/<pid>")]
pub async fn remove_prerequisite(
    principal: AuthedPrincipal,
    pool: &State<MySqlPool>,
    id: &str,
    pid: &str,
) -> AppResult<Json<serde_json::Value>> {
    let p = principal.into_inner();
    let cid = parse_uuid(id, "id")?;
    let pr = parse_uuid(pid, "pid")?;
    course_service::remove_prerequisite(pool.inner(), &p, cid, pr).await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

// ── Templates + export ──────────────────────────────────────────────────────

#[get("/template.csv", rank = 1)]
pub async fn template_csv(_principal: AuthedPrincipal) -> AppResult<BinaryDownload> {
    let bytes = export_service::course_template(ImportFormat::Csv)?;
    Ok(BinaryDownload {
        bytes,
        mime: export_service::mime_for(ImportFormat::Csv),
        filename: export_service::suggested_filename("courses_template", ImportFormat::Csv),
    })
}

#[get("/template.xlsx", rank = 1)]
pub async fn template_xlsx(_principal: AuthedPrincipal) -> AppResult<BinaryDownload> {
    let bytes = export_service::course_template(ImportFormat::Xlsx)?;
    Ok(BinaryDownload {
        bytes,
        mime: export_service::mime_for(ImportFormat::Xlsx),
        filename: export_service::suggested_filename("courses_template", ImportFormat::Xlsx),
    })
}

#[get("/export.csv", rank = 1)]
pub async fn export_csv(
    principal: AuthedPrincipal,
    pool: &State<MySqlPool>,
) -> AppResult<BinaryDownload> {
    let p = principal.into_inner();
    let bytes = export_service::export_courses(pool.inner(), &p, ImportFormat::Csv).await?;
    Ok(BinaryDownload {
        bytes,
        mime: export_service::mime_for(ImportFormat::Csv),
        filename: export_service::suggested_filename("courses", ImportFormat::Csv),
    })
}

#[get("/export.xlsx", rank = 1)]
pub async fn export_xlsx(
    principal: AuthedPrincipal,
    pool: &State<MySqlPool>,
) -> AppResult<BinaryDownload> {
    let p = principal.into_inner();
    let bytes = export_service::export_courses(pool.inner(), &p, ImportFormat::Xlsx).await?;
    Ok(BinaryDownload {
        bytes,
        mime: export_service::mime_for(ImportFormat::Xlsx),
        filename: export_service::suggested_filename("courses", ImportFormat::Xlsx),
    })
}

// ── Import ──────────────────────────────────────────────────────────────────

#[derive(FromForm)]
pub struct CourseImportForm<'r> {
    pub file: TempFile<'r>,
    pub mode: String,
}

#[post("/import?<mode>", data = "<upload>", rank = 1)]
pub async fn import_courses(
    principal: AuthedPrincipal,
    pool: &State<MySqlPool>,
    config: &State<AppConfig>,
    mode: Option<String>,
    mut upload: Form<CourseImportForm<'_>>,
) -> AppResult<Json<ImportReport>> {
    let p = principal.into_inner();

    // Allow mode on either the query string or the form body.
    let mode_s = mode.unwrap_or_else(|| upload.mode.clone());
    let mode_enum = match mode_s.as_str() {
        "dry_run" => ImportMode::DryRun,
        "commit" => ImportMode::Commit,
        other => {
            return Err(AppError::Validation(format!(
                "mode must be 'dry_run' or 'commit', got '{}'",
                other
            )))
        }
    };

    // Detect format from extension (falling back to Content-Type).
    let raw_name = upload
        .file
        .raw_name()
        .map(|n| n.dangerous_unsafe_unsanitized_raw().as_str().to_string())
        .unwrap_or_default();
    let format = ImportFormat::from_extension(&raw_name)
        .or_else(|| {
            upload
                .file
                .content_type()
                .map(|ct| ct.to_string())
                .and_then(|s| ImportFormat::from_mime(&s))
        })
        .ok_or_else(|| {
            AppError::Validation(
                "unable to determine import format (expected .csv or .xlsx)".into(),
            )
        })?;

    // Persist to scratch dir then read. Same strategy as attachments.
    let scratch_dir = std::path::Path::new(&config.attachment_storage_path).join("_scratch");
    tokio::fs::create_dir_all(&scratch_dir)
        .await
        .map_err(|e| AppError::Internal(format!("scratch mkdir: {}", e)))?;
    let scratch_path = scratch_dir.join(Uuid::new_v4().to_string());
    upload
        .file
        .persist_to(&scratch_path)
        .await
        .map_err(|e| AppError::Internal(format!("persist upload: {}", e)))?;
    let bytes = tokio::fs::read(&scratch_path)
        .await
        .map_err(|e| AppError::Internal(format!("read scratch: {}", e)))?;
    let _ = tokio::fs::remove_file(&scratch_path).await;

    let report = import_service::run_course_import(pool.inner(), &p, format, mode_enum, &bytes).await?;
    Ok(Json(report))
}
