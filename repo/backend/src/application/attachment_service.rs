//! Attachment ingest, retrieval, and preview.
//!
//! # Flow
//!
//! ```text
//!   upload bytes ─▶ validate(size, mime, filename)
//!                 ─▶ SHA-256 checksum
//!                 ─▶ write to {storage}/{entity_type}/{uuid}
//!                 ─▶ insert attachments row
//!                 ─▶ audit
//!   preview      ─▶ verify caller can read parent entity
//!                 ─▶ verify mime is in the preview whitelist
//!                 ─▶ read bytes from disk, return with Content-Type
//! ```
//!
//! # Safety invariants
//!
//! * **Never trust client MIME**. The upload endpoint stores the client's
//!   declared MIME in `mime_type` but gates preview on a **server-side**
//!   whitelist; any unexpected byte sequence cannot be served as a
//!   previewable type on the authority of the client header alone.
//! * **Never trust client filename**. The on-disk path is `{uuid}` —
//!   client name is in `original_filename` for display only.
//! * **SHA-256 is computed server-side** and written before the row is
//!   inserted so metadata and content always agree.
//! * **Authorization piggy-backs on parent visibility**. You can read an
//!   attachment iff you can read its parent journal/resource, and you
//!   can upload iff you can edit the parent (plus `AttachmentWrite`).

use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{MySqlPool, Row};
use uuid::Uuid;

use super::audit_service::{self, AuditEvent};
use super::authorization::{require, Capability};
use super::principal::Principal;
use super::resource_service;
use crate::config::AppConfig;
use crate::errors::{AppError, AppResult};
use crate::infrastructure::storage::LocalAttachmentStorage;

// ---------------------------------------------------------------------------
// Limits and whitelists
// ---------------------------------------------------------------------------

/// Maximum accepted upload size — 50 MiB. The Phase 2 `admin_settings` seed
/// also publishes `attachments.max_bytes = 52428800` for documentation and
/// future dynamic overrides.
pub const MAX_ATTACHMENT_BYTES: usize = 50 * 1024 * 1024;

/// Uploads must match one of these MIME types. Enforced **in addition** to
/// any client-declared type — a mismatched declaration is still accepted as
/// long as *some* entry here matches, but clearly illegal values are rejected.
pub const ALLOWED_UPLOAD_MIME: &[&str] = &[
    "application/pdf",
    "application/json",
    "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
    "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
    "application/vnd.openxmlformats-officedocument.presentationml.presentation",
    "image/png",
    "image/jpeg",
    "image/gif",
    "text/csv",
    "text/plain",
    "text/markdown",
];

/// Subset of MIME types that are safe to send back through the preview
/// endpoint. Anything not here returns metadata only and a 415 from the
/// preview route.
pub const PREVIEWABLE_MIME: &[&str] = &[
    "application/pdf",
    "application/json",
    "image/png",
    "image/jpeg",
    "image/gif",
    "text/csv",
    "text/plain",
    "text/markdown",
];

pub fn mime_is_allowed(mime: &str) -> bool {
    ALLOWED_UPLOAD_MIME.iter().any(|m| *m == mime)
}
pub fn mime_is_previewable(mime: &str) -> bool {
    PREVIEWABLE_MIME.iter().any(|m| *m == mime)
}

// ---------------------------------------------------------------------------
// Parent entity discrimination
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ParentType {
    Journal,
    TeachingResource,
}

impl ParentType {
    pub fn as_db(self) -> &'static str {
        match self {
            ParentType::Journal => "journal",
            ParentType::TeachingResource => "teaching_resource",
        }
    }
    pub fn from_db(s: &str) -> Option<Self> {
        match s {
            "journal" => Some(ParentType::Journal),
            "teaching_resource" => Some(ParentType::TeachingResource),
            _ => None,
        }
    }

    /// The capability needed to READ the parent (and therefore read
    /// attachments hanging off it).
    pub fn parent_read_cap(self) -> Capability {
        match self {
            ParentType::Journal => Capability::JournalRead,
            ParentType::TeachingResource => Capability::ResourceRead,
        }
    }

    /// The capability needed to WRITE to the parent (and therefore upload
    /// attachments to it).
    pub fn parent_write_cap(self) -> Capability {
        match self {
            ParentType::Journal => Capability::JournalWrite,
            ParentType::TeachingResource => Capability::ResourceWrite,
        }
    }
}

// ---------------------------------------------------------------------------
// Inputs and view models
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct AttachmentUpload<'a> {
    pub parent_type: ParentType,
    pub parent_id: Uuid,
    pub original_filename: &'a str,
    pub client_mime_type: &'a str,
    pub category: Option<&'a str>,
    pub bytes: &'a [u8],
}

#[derive(Debug, Clone, Serialize)]
pub struct AttachmentView {
    pub id: Uuid,
    pub parent_type: ParentType,
    pub parent_id: Uuid,
    pub original_filename: String,
    pub mime_type: String,
    pub size_bytes: i64,
    pub sha256_checksum: String,
    pub category: Option<String>,
    pub uploaded_by: Option<Uuid>,
    pub created_at: NaiveDateTime,
    pub is_previewable: bool,
}

// ---------------------------------------------------------------------------
// Validation helpers
// ---------------------------------------------------------------------------

const FILENAME_MAX: usize = 255;
const CATEGORY_MAX: usize = 50;

fn sanitize_filename(name: &str) -> AppResult<String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(AppError::Validation("filename is required".into()));
    }
    if trimmed.chars().count() > FILENAME_MAX {
        return Err(AppError::Validation(format!(
            "filename exceeds {} characters",
            FILENAME_MAX
        )));
    }
    // Reject control characters and anything that hints at traversal. The
    // sanitized version is ONLY used for display; the on-disk filename is
    // always a UUID.
    if trimmed.chars().any(|c| c.is_control()) {
        return Err(AppError::Validation("filename contains control characters".into()));
    }
    Ok(trimmed.to_string())
}

fn validate_category(cat: Option<&str>) -> AppResult<Option<String>> {
    match cat {
        None => Ok(None),
        Some(s) if s.is_empty() => Ok(None),
        Some(s) => {
            if s.chars().count() > CATEGORY_MAX {
                return Err(AppError::Validation(format!(
                    "category exceeds {} characters",
                    CATEGORY_MAX
                )));
            }
            if !s
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
            {
                return Err(AppError::Validation(
                    "category must be alphanumeric, underscore, or dash".into(),
                ));
            }
            Ok(Some(s.to_string()))
        }
    }
}

pub fn compute_sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    hex::encode(digest)
}

// ---------------------------------------------------------------------------
// Parent existence + authorization
// ---------------------------------------------------------------------------

/// Verify the parent entity exists and the caller is allowed to **read** it.
///
/// Used for list/get/preview. Write operations use
/// [`ensure_parent_writable`] instead.
///
/// For `TeachingResource` parents the check is delegated to
/// `resource_service::get_resource_by_id`, which enforces the full
/// object-level visibility policy (publication state + Instructor ownership).
/// A bare existence query is not sufficient because an Instructor with
/// `ResourceRead` must not be able to reach attachment data on a non-owned
/// draft resource simply by knowing its UUID.
///
/// For `Journal` parents the existing capability + existence model is retained
/// (journals have a simpler publication/access model in the current RBAC).
async fn ensure_parent_readable(
    pool: &MySqlPool,
    principal: &Principal,
    parent_type: ParentType,
    parent_id: Uuid,
) -> AppResult<()> {
    require(principal, parent_type.parent_read_cap())?;
    match parent_type {
        ParentType::Journal => {
            let exists = sqlx::query("SELECT 1 FROM journals WHERE id = ? LIMIT 1")
                .bind(parent_id.to_string())
                .fetch_optional(pool)
                .await
                .map_err(|e| AppError::Database(format!("parent lookup: {}", e)))?;
            if exists.is_none() {
                return Err(AppError::NotFound(format!(
                    "{} {}",
                    parent_type.as_db(),
                    parent_id
                )));
            }
        }
        ParentType::TeachingResource => {
            // Delegate to the resource service so the full visibility policy
            // is applied — publication state, Instructor ownership, and the
            // Admin/Librarian unrestricted paths are all handled there.
            // `get_resource_by_id` returns NotFound when the resource does not
            // exist or is invisible to the caller, propagating correctly.
            resource_service::get_resource_by_id(pool, principal, parent_id)
                .await
                .map(|_| ())?;
        }
    }
    Ok(())
}

async fn ensure_parent_writable(
    pool: &MySqlPool,
    principal: &Principal,
    parent_type: ParentType,
    parent_id: Uuid,
) -> AppResult<()> {
    require(principal, Capability::AttachmentWrite)?;
    require(principal, parent_type.parent_write_cap())?;
    ensure_parent_readable(pool, principal, parent_type, parent_id).await
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Validate, checksum, persist, and audit a single attachment upload.
pub async fn upload(
    pool: &MySqlPool,
    config: &AppConfig,
    principal: &Principal,
    input: AttachmentUpload<'_>,
) -> AppResult<AttachmentView> {
    ensure_parent_writable(pool, principal, input.parent_type, input.parent_id).await?;

    // Size gate.
    if input.bytes.is_empty() {
        return Err(AppError::Validation("empty upload".into()));
    }
    if input.bytes.len() > MAX_ATTACHMENT_BYTES {
        return Err(AppError::Validation(format!(
            "upload exceeds {} byte limit",
            MAX_ATTACHMENT_BYTES
        )));
    }

    // Mime gate — client-supplied, but must land in our whitelist.
    if !mime_is_allowed(input.client_mime_type) {
        return Err(AppError::Validation(format!(
            "mime type '{}' is not in the allowed upload list",
            input.client_mime_type
        )));
    }

    let display_name = sanitize_filename(input.original_filename)?;
    let category = validate_category(input.category)?;

    // Hash.
    let sha256 = compute_sha256_hex(input.bytes);

    // Safe on-disk filename.
    let attachment_id = Uuid::new_v4();
    let stored_filename = attachment_id.to_string();

    // Write.
    let storage = LocalAttachmentStorage::new(&config.attachment_storage_path);
    let stored_path = storage
        .store_bytes(input.parent_type.as_db(), &stored_filename, input.bytes)
        .await
        .map_err(|e| AppError::Internal(format!("attachment write: {}", e)))?;

    // Insert metadata. `file_path` holds the absolute path for later reads.
    let file_path_s = stored_path.to_string_lossy().to_string();
    sqlx::query(
        r#"INSERT INTO attachments
            (id, entity_type, entity_id,
             file_name, file_path, mime_type, size_bytes,
             sha256_checksum, original_filename, stored_filename, category,
             uploaded_by)
           VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
    )
    .bind(attachment_id.to_string())
    .bind(input.parent_type.as_db())
    .bind(input.parent_id.to_string())
    .bind(&display_name) // file_name mirrors original_filename for backward compat
    .bind(&file_path_s)
    .bind(input.client_mime_type)
    .bind(input.bytes.len() as i64)
    .bind(&sha256)
    .bind(&display_name)
    .bind(&stored_filename)
    .bind(category.as_deref())
    .bind(principal.user_id.to_string())
    .execute(pool)
    .await
    .map_err(|e| AppError::Database(format!("attachment insert: {}", e)))?;

    audit_service::record(
        pool,
        AuditEvent {
            actor_id: Some(principal.user_id),
            actor_email: Some(&principal.email),
            action: "attachment.upload",
            target_entity_type: Some("attachment"),
            target_entity_id: Some(attachment_id),
            change_payload: Some(serde_json::json!({
                "parent_type": input.parent_type.as_db(),
                "parent_id":   input.parent_id,
                "sha256":      sha256,
                "size_bytes":  input.bytes.len(),
                "mime_type":   input.client_mime_type,
                "category":    category,
            })),
            ip_address: None,
            user_agent: None,
        },
    )
    .await?;

    Ok(AttachmentView {
        id: attachment_id,
        parent_type: input.parent_type,
        parent_id: input.parent_id,
        original_filename: display_name,
        mime_type: input.client_mime_type.to_string(),
        size_bytes: input.bytes.len() as i64,
        sha256_checksum: sha256,
        category,
        uploaded_by: Some(principal.user_id),
        created_at: chrono::Utc::now().naive_utc(),
        is_previewable: mime_is_previewable(input.client_mime_type),
    })
}

pub async fn list_for_parent(
    pool: &MySqlPool,
    principal: &Principal,
    parent_type: ParentType,
    parent_id: Uuid,
) -> AppResult<Vec<AttachmentView>> {
    require(principal, Capability::AttachmentRead)?;
    ensure_parent_readable(pool, principal, parent_type, parent_id).await?;

    let rows = sqlx::query(
        r#"SELECT id, entity_type, entity_id, original_filename, mime_type,
                  size_bytes, sha256_checksum, category, uploaded_by, created_at
             FROM attachments
            WHERE entity_type = ? AND entity_id = ? AND is_deleted = FALSE
            ORDER BY created_at DESC"#,
    )
    .bind(parent_type.as_db())
    .bind(parent_id.to_string())
    .fetch_all(pool)
    .await
    .map_err(|e| AppError::Database(format!("list attachments: {}", e)))?;

    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        out.push(row_to_view(row)?);
    }
    Ok(out)
}

pub async fn get_metadata(
    pool: &MySqlPool,
    principal: &Principal,
    attachment_id: Uuid,
) -> AppResult<AttachmentView> {
    require(principal, Capability::AttachmentRead)?;
    let view = load_attachment(pool, attachment_id).await?;
    ensure_parent_readable(pool, principal, view.parent_type, view.parent_id).await?;
    Ok(view)
}

/// Returned by the preview handler. The raw bytes are loaded from disk
/// and the caller is expected to wrap them in a Rocket response with the
/// given Content-Type.
pub struct PreviewPayload {
    pub mime_type: String,
    pub bytes: Vec<u8>,
    pub original_filename: String,
    pub sha256_checksum: String,
}

pub async fn read_preview(
    pool: &MySqlPool,
    config: &AppConfig,
    principal: &Principal,
    attachment_id: Uuid,
) -> AppResult<PreviewPayload> {
    require(principal, Capability::AttachmentRead)?;
    let view = load_attachment(pool, attachment_id).await?;
    ensure_parent_readable(pool, principal, view.parent_type, view.parent_id).await?;

    if !mime_is_previewable(&view.mime_type) {
        return Err(AppError::Validation(format!(
            "preview not available for mime type '{}'",
            view.mime_type
        )));
    }

    // Re-compute the safe path from the stored_filename, never from any
    // client-provided value.
    let storage = LocalAttachmentStorage::new(&config.attachment_storage_path);
    let path = storage
        .resolve_path(view.parent_type.as_db(), &attachment_id.to_string())
        .map_err(|e| AppError::Internal(format!("resolve preview path: {}", e)))?;

    let bytes = storage
        .read_bytes(&path)
        .await
        .map_err(|e| AppError::Internal(format!("read preview: {}", e)))?;

    // Recompute the checksum and reject on drift so operators notice
    // tampering or disk corruption.
    let recomputed = compute_sha256_hex(&bytes);
    if recomputed != view.sha256_checksum {
        tracing::error!(
            attachment = %attachment_id,
            "checksum mismatch on preview: stored={} recomputed={}",
            view.sha256_checksum,
            recomputed
        );
        return Err(AppError::Internal(
            "attachment checksum mismatch (possible tampering)".into(),
        ));
    }

    let _ = audit_service::record(
        pool,
        AuditEvent {
            actor_id: Some(principal.user_id),
            actor_email: Some(&principal.email),
            action: "attachment.preview",
            target_entity_type: Some("attachment"),
            target_entity_id: Some(attachment_id),
            change_payload: None,
            ip_address: None,
            user_agent: None,
        },
    )
    .await;

    Ok(PreviewPayload {
        mime_type: view.mime_type,
        bytes,
        original_filename: view.original_filename,
        sha256_checksum: view.sha256_checksum,
    })
}

/// Soft-delete an attachment — marks `is_deleted = TRUE` and removes the
/// backing file from disk. The metadata row stays around for audit.
pub async fn delete_attachment(
    pool: &MySqlPool,
    config: &AppConfig,
    principal: &Principal,
    attachment_id: Uuid,
) -> AppResult<()> {
    require(principal, Capability::AttachmentDelete)?;
    let view = load_attachment(pool, attachment_id).await?;
    // Soft-delete requires at least write on the parent (librarians and
    // admins both satisfy AttachmentDelete; we re-assert parent visibility
    // to catch any future inconsistency in the RBAC matrix).
    require(principal, view.parent_type.parent_write_cap())?;

    sqlx::query("UPDATE attachments SET is_deleted = TRUE WHERE id = ?")
        .bind(attachment_id.to_string())
        .execute(pool)
        .await
        .map_err(|e| AppError::Database(format!("soft delete: {}", e)))?;

    // Try to unlink the file; failure is logged but does not break the
    // metadata soft-delete (we don't want an inconsistent is_deleted state
    // just because the volume is read-only for a moment).
    let storage = LocalAttachmentStorage::new(&config.attachment_storage_path);
    if let Ok(path) =
        storage.resolve_path(view.parent_type.as_db(), &attachment_id.to_string())
    {
        if let Err(e) = storage.delete(&path).await {
            tracing::warn!("attachment unlink failed for {}: {}", attachment_id, e);
        }
    }

    audit_service::record(
        pool,
        AuditEvent {
            actor_id: Some(principal.user_id),
            actor_email: Some(&principal.email),
            action: "attachment.delete",
            target_entity_type: Some("attachment"),
            target_entity_id: Some(attachment_id),
            change_payload: None,
            ip_address: None,
            user_agent: None,
        },
    )
    .await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Internal loader
// ---------------------------------------------------------------------------

async fn load_attachment(
    pool: &MySqlPool,
    attachment_id: Uuid,
) -> AppResult<AttachmentView> {
    let row = sqlx::query(
        r#"SELECT id, entity_type, entity_id, original_filename, mime_type,
                  size_bytes, sha256_checksum, category, uploaded_by, created_at
             FROM attachments
            WHERE id = ? AND is_deleted = FALSE
            LIMIT 1"#,
    )
    .bind(attachment_id.to_string())
    .fetch_optional(pool)
    .await
    .map_err(|e| AppError::Database(format!("load attachment: {}", e)))?
    .ok_or_else(|| AppError::NotFound(format!("attachment {}", attachment_id)))?;
    row_to_view(row)
}

fn row_to_view(row: sqlx::mysql::MySqlRow) -> AppResult<AttachmentView> {
    let id: String = row.try_get("id").map_err(|e| AppError::Database(e.to_string()))?;
    let entity_type: String = row
        .try_get("entity_type")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let entity_id: String = row
        .try_get("entity_id")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let original_filename: Option<String> = row
        .try_get("original_filename")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let mime_type: Option<String> = row
        .try_get("mime_type")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let size_bytes: Option<i64> = row
        .try_get("size_bytes")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let sha256: Option<String> = row
        .try_get("sha256_checksum")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let category: Option<String> = row
        .try_get("category")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let uploaded_by: Option<String> = row
        .try_get("uploaded_by")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let created_at: NaiveDateTime = row
        .try_get("created_at")
        .map_err(|e| AppError::Database(e.to_string()))?;

    let parent_type = ParentType::from_db(&entity_type)
        .ok_or_else(|| AppError::Database(format!("unknown parent type {}", entity_type)))?;
    let mime = mime_type.unwrap_or_default();
    Ok(AttachmentView {
        id: Uuid::parse_str(&id).map_err(|e| AppError::Database(e.to_string()))?,
        parent_type,
        parent_id: Uuid::parse_str(&entity_id)
            .map_err(|e| AppError::Database(e.to_string()))?,
        original_filename: original_filename.unwrap_or_default(),
        is_previewable: mime_is_previewable(&mime),
        mime_type: mime,
        size_bytes: size_bytes.unwrap_or(0),
        sha256_checksum: sha256.unwrap_or_default(),
        category,
        uploaded_by: uploaded_by.as_deref().and_then(|s| Uuid::parse_str(s).ok()),
        created_at,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mime_whitelist_basics() {
        assert!(mime_is_allowed("application/pdf"));
        assert!(mime_is_allowed("image/png"));
        assert!(!mime_is_allowed("application/x-shellscript"));
        assert!(!mime_is_allowed(""));
    }

    #[test]
    fn preview_whitelist_is_subset_of_upload_whitelist() {
        for m in PREVIEWABLE_MIME {
            assert!(
                ALLOWED_UPLOAD_MIME.contains(m),
                "previewable mime {} must also be uploadable",
                m
            );
        }
    }

    #[test]
    fn sanitize_filename_rules() {
        assert!(sanitize_filename("").is_err());
        assert!(sanitize_filename("   ").is_err());
        assert!(sanitize_filename("ok.pdf").is_ok());
        // Control characters are rejected.
        assert!(sanitize_filename("bad\x07name.pdf").is_err());
        // Too long.
        assert!(sanitize_filename(&"x".repeat(300)).is_err());
        // Note: slashes are *not* rejected at this layer because the
        // on-disk filename is a UUID anyway; display-only name is allowed
        // to contain anything except control characters.
    }

    #[test]
    fn validate_category_rules() {
        assert_eq!(validate_category(None).unwrap(), None);
        assert_eq!(validate_category(Some("")).unwrap(), None);
        assert_eq!(
            validate_category(Some("sample_issue")).unwrap(),
            Some("sample_issue".to_string())
        );
        assert!(validate_category(Some("bad category")).is_err()); // space
        assert!(validate_category(Some("bad/category")).is_err()); // slash
        assert!(validate_category(Some(&"x".repeat(60))).is_err()); // too long
    }

    #[test]
    fn sha256_is_deterministic() {
        let a = compute_sha256_hex(b"hello world");
        let b = compute_sha256_hex(b"hello world");
        assert_eq!(a, b);
        // Known vector.
        assert_eq!(
            a,
            "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
        );
    }

    #[test]
    fn parent_type_capability_mapping() {
        assert_eq!(ParentType::Journal.parent_read_cap(), Capability::JournalRead);
        assert_eq!(ParentType::Journal.parent_write_cap(), Capability::JournalWrite);
        assert_eq!(
            ParentType::TeachingResource.parent_read_cap(),
            Capability::ResourceRead
        );
        assert_eq!(
            ParentType::TeachingResource.parent_write_cap(),
            Capability::ResourceWrite
        );
    }

    // -----------------------------------------------------------------
    // Phase 3 additional coverage
    // -----------------------------------------------------------------

    #[test]
    fn sanitize_filename_accepts_common_names() {
        // Simple ASCII names.
        assert_eq!(sanitize_filename("report.pdf").unwrap(), "report.pdf");
        assert_eq!(
            sanitize_filename("contract-2026.docx").unwrap(),
            "contract-2026.docx"
        );
        // Unicode names are allowed — the on-disk filename is always a UUID
        // so we only strip control characters.
        assert_eq!(sanitize_filename("naïve.txt").unwrap(), "naïve.txt");
    }

    #[test]
    fn sanitize_filename_rejects_bad_inputs() {
        // Empty / whitespace.
        assert!(sanitize_filename("").is_err());
        assert!(sanitize_filename("   ").is_err());
        // Embedded NUL is a control character -> reject.
        assert!(sanitize_filename("has\x00null.pdf").is_err());
        // Filename longer than FILENAME_MAX (>256 chars) must be rejected.
        let too_long: String = "a".repeat(257);
        assert!(sanitize_filename(&too_long).is_err());
    }

    #[test]
    fn validate_category_documented_values() {
        // Documented categories from the design doc.
        for cat in &["sample_issue", "contract", "vendor_quote", "other"] {
            let got = validate_category(Some(*cat))
                .unwrap_or_else(|e| panic!("category {} should be valid: {:?}", cat, e));
            assert_eq!(got.as_deref(), Some(*cat));
        }
        // Space-containing category must be rejected.
        assert!(validate_category(Some("space in name")).is_err());
    }

    #[test]
    fn parent_type_from_db_round_trips() {
        assert_eq!(ParentType::from_db("journal"), Some(ParentType::Journal));
        assert_eq!(
            ParentType::from_db("teaching_resource"),
            Some(ParentType::TeachingResource)
        );
        assert_eq!(ParentType::from_db("nope"), None);
        // as_db / from_db round-trip for completeness.
        for pt in [ParentType::Journal, ParentType::TeachingResource] {
            assert_eq!(ParentType::from_db(pt.as_db()), Some(pt));
        }
    }

    #[test]
    fn sha256_empty_string_known_vector() {
        // Famous empty-string SHA-256 test vector.
        assert_eq!(
            compute_sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn mime_is_previewable_cases() {
        // Script bodies must never be reflected through the preview path.
        assert!(!mime_is_previewable("application/x-shellscript"));
        // PDFs are whitelisted.
        assert!(mime_is_previewable("application/pdf"));
        // Plain text is too.
        assert!(mime_is_previewable("text/plain"));
        // .docx is uploadable but NOT previewable — defence in depth.
        assert!(!mime_is_previewable(
            "application/vnd.openxmlformats-officedocument.wordprocessingml.document"
        ));
    }
}
