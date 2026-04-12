//! Section REST routes — Phase 4 implementation.
//!
//! Routes under `/api/v1/sections`:
//!
//! * CRUD + versioning: list, create, get, new-draft, list_versions,
//!   approve, publish
//! * Templates: `/template.csv`, `/template.xlsx`
//! * Export:    `/export.csv`,   `/export.xlsx`
//! * Import:    `POST /import`

use rocket::form::Form;
use rocket::fs::TempFile;
use rocket::serde::json::Json;
use rocket::State;
use sqlx::MySqlPool;
use uuid::Uuid;

use crate::api::download::BinaryDownload;
use crate::api::guards::AuthedPrincipal;
use crate::application::encryption::FieldEncryption;
use crate::application::export_service;
use crate::application::import_service::{
    self, ImportFormat, ImportMode, ImportReport,
};
use crate::application::section_service::{
    self, SectionCreateInput, SectionEditInput, SectionVersionView, SectionView,
};
use crate::config::AppConfig;
use crate::errors::{AppError, AppResult};

pub fn routes() -> Vec<rocket::Route> {
    routes![
        list_sections,
        create_section,
        get_section,
        new_draft,
        list_versions,
        approve_version,
        publish_version,
        template_csv,
        template_xlsx,
        export_csv,
        export_xlsx,
        import_sections,
    ]
}

fn parse_uuid(s: &str, field: &str) -> AppResult<Uuid> {
    Uuid::parse_str(s).map_err(|_| AppError::Validation(format!("{} must be a UUID", field)))
}

#[get("/?<course_id>&<department_id>&<limit>&<offset>")]
pub async fn list_sections(
    principal: AuthedPrincipal,
    pool: &State<MySqlPool>,
    enc: &State<FieldEncryption>,
    course_id: Option<String>,
    department_id: Option<String>,
    limit: Option<u32>,
    offset: Option<u32>,
) -> AppResult<Json<Vec<SectionView>>> {
    let p = principal.into_inner();
    let cid = match course_id {
        Some(ref s) if !s.is_empty() => Some(parse_uuid(s, "course_id")?),
        _ => None,
    };
    let dept = match department_id {
        Some(ref s) if !s.is_empty() => Some(parse_uuid(s, "department_id")?),
        _ => None,
    };
    let views = section_service::list_sections(
        pool.inner(),
        &p,
        cid,
        dept,
        limit.unwrap_or(100),
        offset.unwrap_or(0),
        enc.inner(),
    )
    .await?;
    Ok(Json(views))
}

#[post("/", format = "json", data = "<body>")]
pub async fn create_section(
    principal: AuthedPrincipal,
    pool: &State<MySqlPool>,
    enc: &State<FieldEncryption>,
    body: Json<SectionCreateInput>,
) -> AppResult<Json<SectionView>> {
    let p = principal.into_inner();
    Ok(Json(
        section_service::create_section(pool.inner(), &p, body.into_inner(), enc.inner()).await?,
    ))
}

#[get("/<id>", rank = 2)]
pub async fn get_section(
    principal: AuthedPrincipal,
    pool: &State<MySqlPool>,
    enc: &State<FieldEncryption>,
    id: &str,
) -> AppResult<Json<SectionView>> {
    let p = principal.into_inner();
    let sid = parse_uuid(id, "id")?;
    Ok(Json(
        section_service::get_section_by_id(pool.inner(), &p, sid, enc.inner()).await?,
    ))
}

#[put("/<id>", format = "json", data = "<body>")]
pub async fn new_draft(
    principal: AuthedPrincipal,
    pool: &State<MySqlPool>,
    enc: &State<FieldEncryption>,
    id: &str,
    body: Json<SectionEditInput>,
) -> AppResult<Json<SectionVersionView>> {
    let p = principal.into_inner();
    let sid = parse_uuid(id, "id")?;
    Ok(Json(
        section_service::create_draft_version(pool.inner(), &p, sid, body.into_inner(), enc.inner()).await?,
    ))
}

#[get("/<id>/versions")]
pub async fn list_versions(
    principal: AuthedPrincipal,
    pool: &State<MySqlPool>,
    enc: &State<FieldEncryption>,
    id: &str,
) -> AppResult<Json<Vec<SectionVersionView>>> {
    let p = principal.into_inner();
    let sid = parse_uuid(id, "id")?;
    Ok(Json(
        section_service::list_versions(pool.inner(), &p, sid, enc.inner()).await?,
    ))
}

#[post("/<id>/versions/<vid>/approve")]
pub async fn approve_version(
    principal: AuthedPrincipal,
    pool: &State<MySqlPool>,
    enc: &State<FieldEncryption>,
    id: &str,
    vid: &str,
) -> AppResult<Json<SectionVersionView>> {
    let p = principal.into_inner();
    let sid = parse_uuid(id, "id")?;
    let v = parse_uuid(vid, "vid")?;
    Ok(Json(
        section_service::approve_version(pool.inner(), &p, sid, v, enc.inner()).await?,
    ))
}

#[post("/<id>/versions/<vid>/publish")]
pub async fn publish_version(
    principal: AuthedPrincipal,
    pool: &State<MySqlPool>,
    enc: &State<FieldEncryption>,
    id: &str,
    vid: &str,
) -> AppResult<Json<SectionView>> {
    let p = principal.into_inner();
    let sid = parse_uuid(id, "id")?;
    let v = parse_uuid(vid, "vid")?;
    Ok(Json(
        section_service::publish_version(pool.inner(), &p, sid, v, enc.inner()).await?,
    ))
}

// ── Templates + export ──────────────────────────────────────────────────────

#[get("/template.csv", rank = 1)]
pub async fn template_csv(_principal: AuthedPrincipal) -> AppResult<BinaryDownload> {
    let bytes = export_service::section_template(ImportFormat::Csv)?;
    Ok(BinaryDownload {
        bytes,
        mime: export_service::mime_for(ImportFormat::Csv),
        filename: export_service::suggested_filename("sections_template", ImportFormat::Csv),
    })
}

#[get("/template.xlsx", rank = 1)]
pub async fn template_xlsx(_principal: AuthedPrincipal) -> AppResult<BinaryDownload> {
    let bytes = export_service::section_template(ImportFormat::Xlsx)?;
    Ok(BinaryDownload {
        bytes,
        mime: export_service::mime_for(ImportFormat::Xlsx),
        filename: export_service::suggested_filename("sections_template", ImportFormat::Xlsx),
    })
}

#[get("/export.csv", rank = 1)]
pub async fn export_csv(
    principal: AuthedPrincipal,
    pool: &State<MySqlPool>,
) -> AppResult<BinaryDownload> {
    let p = principal.into_inner();
    let bytes = export_service::export_sections(pool.inner(), &p, ImportFormat::Csv).await?;
    Ok(BinaryDownload {
        bytes,
        mime: export_service::mime_for(ImportFormat::Csv),
        filename: export_service::suggested_filename("sections", ImportFormat::Csv),
    })
}

#[get("/export.xlsx", rank = 1)]
pub async fn export_xlsx(
    principal: AuthedPrincipal,
    pool: &State<MySqlPool>,
) -> AppResult<BinaryDownload> {
    let p = principal.into_inner();
    let bytes = export_service::export_sections(pool.inner(), &p, ImportFormat::Xlsx).await?;
    Ok(BinaryDownload {
        bytes,
        mime: export_service::mime_for(ImportFormat::Xlsx),
        filename: export_service::suggested_filename("sections", ImportFormat::Xlsx),
    })
}

// ── Import ──────────────────────────────────────────────────────────────────

#[derive(FromForm)]
pub struct SectionImportForm<'r> {
    pub file: TempFile<'r>,
    pub mode: String,
}

#[post("/import?<mode>", data = "<upload>", rank = 1)]
pub async fn import_sections(
    principal: AuthedPrincipal,
    pool: &State<MySqlPool>,
    config: &State<AppConfig>,
    mode: Option<String>,
    mut upload: Form<SectionImportForm<'_>>,
) -> AppResult<Json<ImportReport>> {
    let p = principal.into_inner();
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

    let report =
        import_service::run_section_import(pool.inner(), &p, format, mode_enum, &bytes).await?;
    Ok(Json(report))
}
