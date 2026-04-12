//! Attachment REST routes — Phase 3.
//!
//! Multipart upload uses Rocket's `Form<Upload<'_>>` with a `TempFile`
//! backing. The handler reads the temp file from disk, hashes, and hands
//! the bytes to `attachment_service::upload` which performs all of the
//! validation + persistence work.
//!
//! The preview endpoint streams whitelisted binary types with the correct
//! `Content-Type`; unsupported types return a 422 via the error envelope.

use std::io::Cursor;

use rocket::form::Form;
use rocket::fs::TempFile;
use rocket::http::{ContentType, Status};
use rocket::response::{self, Responder, Response};
use rocket::serde::json::Json;
use rocket::{Request, State};
use sqlx::MySqlPool;
use uuid::Uuid;

use crate::api::guards::AuthedPrincipal;
use crate::application::attachment_service::{
    self, AttachmentUpload, AttachmentView, ParentType,
};
use crate::config::AppConfig;
use crate::errors::{AppError, AppResult};

pub fn routes() -> Vec<rocket::Route> {
    routes![list_attachments, upload_attachment, get_attachment, preview_attachment, delete_attachment]
}

fn parse_uuid(s: &str, field: &str) -> AppResult<Uuid> {
    Uuid::parse_str(s).map_err(|_| AppError::Validation(format!("{} must be a UUID", field)))
}

fn parse_parent_type(s: &str) -> AppResult<ParentType> {
    ParentType::from_db(s).ok_or_else(|| {
        AppError::Validation(format!(
            "parent_type must be 'journal' or 'teaching_resource', got '{}'",
            s
        ))
    })
}

// ---------------------------------------------------------------------------
// Upload
// ---------------------------------------------------------------------------

#[derive(FromForm)]
pub struct UploadForm<'r> {
    pub file: TempFile<'r>,
    pub parent_type: String,
    pub parent_id: String,
    pub category: Option<String>,
}

#[post("/", data = "<upload>")]
pub async fn upload_attachment(
    principal: AuthedPrincipal,
    pool: &State<MySqlPool>,
    config: &State<AppConfig>,
    mut upload: Form<UploadForm<'_>>,
) -> AppResult<Json<AttachmentView>> {
    let p = principal.into_inner();

    let parent_type = parse_parent_type(&upload.parent_type)?;
    let parent_id = parse_uuid(&upload.parent_id, "parent_id")?;

    let client_mime = upload
        .file
        .content_type()
        .map(|c| c.to_string())
        .unwrap_or_else(|| "application/octet-stream".into());

    let original_filename = upload
        .file
        .raw_name()
        .map(|n| n.dangerous_unsafe_unsanitized_raw().as_str().to_string())
        .unwrap_or_else(|| "upload.bin".into());

    // Read the temp file into memory so we can hash + validate in one
    // place. The service layer enforces the size cap.
    //
    // Rocket's TempFile is either an on-disk file OR an in-memory buffer;
    // in both cases `persist_to` moves it to a destination, and `path()`
    // (available after buffering to disk) returns the on-disk location.
    // The simplest cross-mode strategy: persist to a scratch path under
    // the storage volume, read bytes, then let the service write the
    // permanent copy. That's one extra file-read per upload — acceptable
    // for the sizes we accept (<= 50 MiB).
    let scratch_dir = std::path::Path::new(&config.attachment_storage_path).join("_scratch");
    tokio::fs::create_dir_all(&scratch_dir)
        .await
        .map_err(|e| AppError::Internal(format!("scratch mkdir: {}", e)))?;
    let scratch_name = Uuid::new_v4().to_string();
    let scratch_path = scratch_dir.join(&scratch_name);

    upload
        .file
        .persist_to(&scratch_path)
        .await
        .map_err(|e| AppError::Internal(format!("persist upload: {}", e)))?;

    let bytes = tokio::fs::read(&scratch_path)
        .await
        .map_err(|e| AppError::Internal(format!("read scratch: {}", e)))?;

    let view_result = attachment_service::upload(
        pool.inner(),
        config.inner(),
        &p,
        AttachmentUpload {
            parent_type,
            parent_id,
            original_filename: &original_filename,
            client_mime_type: &client_mime,
            category: upload.category.as_deref(),
            bytes: &bytes,
        },
    )
    .await;

    // Clean up the scratch copy either way.
    let _ = tokio::fs::remove_file(&scratch_path).await;

    Ok(Json(view_result?))
}

// ---------------------------------------------------------------------------
// Read paths
// ---------------------------------------------------------------------------

#[get("/?<parent_type>&<parent_id>")]
pub async fn list_attachments(
    principal: AuthedPrincipal,
    pool: &State<MySqlPool>,
    parent_type: String,
    parent_id: String,
) -> AppResult<Json<Vec<AttachmentView>>> {
    let p = principal.into_inner();
    let pt = parse_parent_type(&parent_type)?;
    let pid = parse_uuid(&parent_id, "parent_id")?;
    let views = attachment_service::list_for_parent(pool.inner(), &p, pt, pid).await?;
    Ok(Json(views))
}

#[get("/<id>")]
pub async fn get_attachment(
    principal: AuthedPrincipal,
    pool: &State<MySqlPool>,
    id: &str,
) -> AppResult<Json<AttachmentView>> {
    let p = principal.into_inner();
    let aid = parse_uuid(id, "id")?;
    let view = attachment_service::get_metadata(pool.inner(), &p, aid).await?;
    Ok(Json(view))
}

// ---------------------------------------------------------------------------
// Preview — returns raw bytes with the original Content-Type
// ---------------------------------------------------------------------------

/// Rocket responder that streams a byte vector with a specific Content-Type.
pub struct PreviewBytes {
    pub mime: String,
    pub bytes: Vec<u8>,
    pub sha256: String,
    pub filename: String,
}

impl<'r> Responder<'r, 'static> for PreviewBytes {
    fn respond_to(self, _req: &'r Request<'_>) -> response::Result<'static> {
        let content_type = ContentType::parse_flexible(&self.mime)
            .unwrap_or(ContentType::Binary);
        Response::build()
            .status(Status::Ok)
            .header(content_type)
            .raw_header(
                "X-Attachment-Checksum",
                format!("sha256:{}", self.sha256),
            )
            .raw_header(
                "X-Attachment-Filename",
                // X-headers should be ASCII; strip non-ASCII just in case.
                self.filename
                    .chars()
                    .filter(|c| c.is_ascii() && !c.is_control())
                    .collect::<String>(),
            )
            .sized_body(self.bytes.len(), Cursor::new(self.bytes))
            .ok()
    }
}

#[get("/<id>/preview")]
pub async fn preview_attachment(
    principal: AuthedPrincipal,
    pool: &State<MySqlPool>,
    config: &State<AppConfig>,
    id: &str,
) -> AppResult<PreviewBytes> {
    let p = principal.into_inner();
    let aid = parse_uuid(id, "id")?;
    let payload =
        attachment_service::read_preview(pool.inner(), config.inner(), &p, aid).await?;
    Ok(PreviewBytes {
        mime: payload.mime_type,
        bytes: payload.bytes,
        sha256: payload.sha256_checksum,
        filename: payload.original_filename,
    })
}

// ---------------------------------------------------------------------------
// Delete
// ---------------------------------------------------------------------------

#[delete("/<id>")]
pub async fn delete_attachment(
    principal: AuthedPrincipal,
    pool: &State<MySqlPool>,
    config: &State<AppConfig>,
    id: &str,
) -> AppResult<Json<serde_json::Value>> {
    let p = principal.into_inner();
    let aid = parse_uuid(id, "id")?;
    attachment_service::delete_attachment(pool.inner(), config.inner(), &p, aid).await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}
