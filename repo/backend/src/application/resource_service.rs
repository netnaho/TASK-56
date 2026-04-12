//! Teaching resource CRUD + versioning workflow.
//!
//! Mirrors the shape of `journal_service` (draft → approved → published
//! state machine, two transactional pointers on the parent row) with one
//! domain addition: each resource carries a `resource_type` enum
//! (`document`, `video`, `presentation`, `assessment`, `external_link`,
//! `dataset`, `other`).
//!
//! See the module-level docs on [`super::journal_service`] for the
//! detailed workflow reasoning; the two services intentionally share
//! vocabulary so the UI can treat them uniformly.

use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use sqlx::{MySqlPool, Row};
use uuid::Uuid;

use super::audit_service::{self, AuditEvent};
use super::authorization::{require, Capability};
use super::principal::{Principal, Role};
use crate::domain::versioning::{validate_transition, VersionState};
use crate::errors::{AppError, AppResult};

// ---------------------------------------------------------------------------
// ResourceType — mirrors teaching_resources.resource_type ENUM
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceType {
    Document,
    Video,
    Presentation,
    Assessment,
    ExternalLink,
    Dataset,
    Other,
}

impl ResourceType {
    pub fn as_db(self) -> &'static str {
        match self {
            ResourceType::Document => "document",
            ResourceType::Video => "video",
            ResourceType::Presentation => "presentation",
            ResourceType::Assessment => "assessment",
            ResourceType::ExternalLink => "external_link",
            ResourceType::Dataset => "dataset",
            ResourceType::Other => "other",
        }
    }
    pub fn from_db(s: &str) -> Option<Self> {
        match s {
            "document" => Some(ResourceType::Document),
            "video" => Some(ResourceType::Video),
            "presentation" => Some(ResourceType::Presentation),
            "assessment" => Some(ResourceType::Assessment),
            "external_link" => Some(ResourceType::ExternalLink),
            "dataset" => Some(ResourceType::Dataset),
            "other" => Some(ResourceType::Other),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// Inputs and view models
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct ResourceCreateInput {
    pub title: String,
    pub resource_type: ResourceType,
    pub description: Option<String>,
    pub content_url: Option<String>,
    pub mime_type: Option<String>,
    pub change_summary: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ResourceEditInput {
    pub description: Option<String>,
    pub content_url: Option<String>,
    pub mime_type: Option<String>,
    pub change_summary: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ResourceView {
    pub id: Uuid,
    pub title: String,
    pub resource_type: ResourceType,
    pub owner_id: Option<Uuid>,
    pub is_published: bool,
    pub current_version_id: Option<Uuid>,
    pub latest_version_id: Option<Uuid>,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
    pub effective_version: Option<ResourceVersionView>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ResourceVersionView {
    pub id: Uuid,
    pub resource_id: Uuid,
    pub version_number: i32,
    pub content_url: Option<String>,
    pub mime_type: Option<String>,
    pub size_bytes: Option<i64>,
    pub description: Option<String>,
    pub change_summary: Option<String>,
    pub state: VersionState,
    pub created_by: Option<Uuid>,
    pub created_at: NaiveDateTime,
    pub approved_by: Option<Uuid>,
    pub approved_at: Option<NaiveDateTime>,
    pub published_by: Option<Uuid>,
    pub published_at: Option<NaiveDateTime>,
}

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

const TITLE_MIN: usize = 3;
const TITLE_MAX: usize = 500;
const URL_MAX: usize = 2_000;
const MIME_MAX: usize = 255;
const DESCRIPTION_MAX: usize = 4_000;
const CHANGE_SUMMARY_MAX: usize = 2_000;

fn validate_title(title: &str) -> AppResult<()> {
    let len = title.trim().chars().count();
    if len < TITLE_MIN {
        return Err(AppError::Validation(format!(
            "title must be at least {} characters",
            TITLE_MIN
        )));
    }
    if len > TITLE_MAX {
        return Err(AppError::Validation(format!(
            "title must be at most {} characters",
            TITLE_MAX
        )));
    }
    Ok(())
}

fn validate_optional(field: &str, value: Option<&str>, max: usize) -> AppResult<()> {
    if let Some(v) = value {
        if v.chars().count() > max {
            return Err(AppError::Validation(format!(
                "{} exceeds {} character limit",
                field, max
            )));
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

pub async fn create_resource(
    pool: &MySqlPool,
    principal: &Principal,
    input: ResourceCreateInput,
) -> AppResult<ResourceView> {
    require(principal, Capability::ResourceWrite)?;
    validate_title(&input.title)?;
    validate_optional("description", input.description.as_deref(), DESCRIPTION_MAX)?;
    validate_optional("content_url", input.content_url.as_deref(), URL_MAX)?;
    validate_optional("mime_type", input.mime_type.as_deref(), MIME_MAX)?;
    validate_optional(
        "change_summary",
        input.change_summary.as_deref(),
        CHANGE_SUMMARY_MAX,
    )?;

    let mut tx = pool
        .begin()
        .await
        .map_err(|e| AppError::Database(format!("create_resource tx: {}", e)))?;

    let resource_id = Uuid::new_v4();
    let version_id = Uuid::new_v4();

    sqlx::query(
        r#"INSERT INTO teaching_resources
           (id, owner_id, title, resource_type, is_published)
           VALUES (?, ?, ?, ?, FALSE)"#,
    )
    .bind(resource_id.to_string())
    .bind(principal.user_id.to_string())
    .bind(input.title.trim())
    .bind(input.resource_type.as_db())
    .execute(&mut *tx)
    .await
    .map_err(|e| AppError::Database(format!("insert resource: {}", e)))?;

    sqlx::query(
        r#"INSERT INTO resource_versions
           (id, resource_id, version_number, content_url, mime_type, description,
            change_summary, state, created_by)
           VALUES (?, ?, 1, ?, ?, ?, ?, 'draft', ?)"#,
    )
    .bind(version_id.to_string())
    .bind(resource_id.to_string())
    .bind(input.content_url.as_deref())
    .bind(input.mime_type.as_deref())
    .bind(input.description.as_deref())
    .bind(input.change_summary.as_deref())
    .bind(principal.user_id.to_string())
    .execute(&mut *tx)
    .await
    .map_err(|e| AppError::Database(format!("insert version: {}", e)))?;

    sqlx::query("UPDATE teaching_resources SET latest_version_id = ? WHERE id = ?")
        .bind(version_id.to_string())
        .bind(resource_id.to_string())
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::Database(format!("set latest: {}", e)))?;

    tx.commit()
        .await
        .map_err(|e| AppError::Database(format!("create_resource commit: {}", e)))?;

    audit_service::record(
        pool,
        AuditEvent {
            actor_id: Some(principal.user_id),
            actor_email: Some(&principal.email),
            action: "resource.create",
            target_entity_type: Some("teaching_resource"),
            target_entity_id: Some(resource_id),
            change_payload: Some(serde_json::json!({
                "version_id": version_id,
                "title": input.title.trim(),
                "resource_type": input.resource_type.as_db(),
            })),
            ip_address: None,
            user_agent: None,
        },
    )
    .await?;

    get_resource_by_id(pool, principal, resource_id).await
}

pub async fn create_draft_version(
    pool: &MySqlPool,
    principal: &Principal,
    resource_id: Uuid,
    input: ResourceEditInput,
) -> AppResult<ResourceVersionView> {
    require(principal, Capability::ResourceWrite)?;

    // Instructors may only create draft versions for resources they personally
    // own.  Admins and Librarians have unrestricted write access.
    if !principal.is_admin() && !principal.has_role(Role::Librarian) {
        let owner_row = sqlx::query(
            "SELECT owner_id FROM teaching_resources WHERE id = ? LIMIT 1",
        )
        .bind(resource_id.to_string())
        .fetch_optional(pool)
        .await
        .map_err(|e| AppError::Database(format!("check resource owner: {}", e)))?
        .ok_or_else(|| AppError::NotFound(format!("teaching_resource {}", resource_id)))?;
        let owner_s: Option<String> = owner_row
            .try_get("owner_id")
            .map_err(|e| AppError::Database(e.to_string()))?;
        let owner_uuid = owner_s.as_deref().and_then(|s| Uuid::parse_str(s).ok());
        if owner_uuid != Some(principal.user_id) {
            return Err(AppError::Forbidden);
        }
    }

    validate_optional("description", input.description.as_deref(), DESCRIPTION_MAX)?;
    validate_optional("content_url", input.content_url.as_deref(), URL_MAX)?;
    validate_optional("mime_type", input.mime_type.as_deref(), MIME_MAX)?;
    validate_optional(
        "change_summary",
        input.change_summary.as_deref(),
        CHANGE_SUMMARY_MAX,
    )?;

    let mut tx = pool
        .begin()
        .await
        .map_err(|e| AppError::Database(format!("draft tx: {}", e)))?;

    let max_row = sqlx::query(
        r#"SELECT COALESCE(MAX(version_number), 0) AS max_ver
             FROM resource_versions
            WHERE resource_id = ?
            FOR UPDATE"#,
    )
    .bind(resource_id.to_string())
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| AppError::Database(format!("max ver: {}", e)))?;
    let max_ver: i32 = max_row
        .try_get("max_ver")
        .map_err(|e| AppError::Database(e.to_string()))?;
    if max_ver == 0 {
        return Err(AppError::NotFound(format!("teaching_resource {}", resource_id)));
    }
    let new_ver = max_ver + 1;
    let version_id = Uuid::new_v4();

    sqlx::query(
        r#"INSERT INTO resource_versions
           (id, resource_id, version_number, content_url, mime_type, description,
            change_summary, state, created_by)
           VALUES (?, ?, ?, ?, ?, ?, ?, 'draft', ?)"#,
    )
    .bind(version_id.to_string())
    .bind(resource_id.to_string())
    .bind(new_ver)
    .bind(input.content_url.as_deref())
    .bind(input.mime_type.as_deref())
    .bind(input.description.as_deref())
    .bind(input.change_summary.as_deref())
    .bind(principal.user_id.to_string())
    .execute(&mut *tx)
    .await
    .map_err(|e| AppError::Database(format!("insert draft: {}", e)))?;

    sqlx::query("UPDATE teaching_resources SET latest_version_id = ? WHERE id = ?")
        .bind(version_id.to_string())
        .bind(resource_id.to_string())
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::Database(format!("update latest: {}", e)))?;

    tx.commit()
        .await
        .map_err(|e| AppError::Database(format!("draft commit: {}", e)))?;

    audit_service::record(
        pool,
        AuditEvent {
            actor_id: Some(principal.user_id),
            actor_email: Some(&principal.email),
            action: "resource.draft.create",
            target_entity_type: Some("resource_version"),
            target_entity_id: Some(version_id),
            change_payload: Some(serde_json::json!({
                "resource_id": resource_id,
                "version_number": new_ver,
            })),
            ip_address: None,
            user_agent: None,
        },
    )
    .await?;

    load_version_by_id(pool, version_id).await
}

pub async fn approve_version(
    pool: &MySqlPool,
    principal: &Principal,
    resource_id: Uuid,
    version_id: Uuid,
) -> AppResult<ResourceVersionView> {
    require(principal, Capability::ResourceApprove)?;
    let existing = load_version_by_id(pool, version_id).await?;
    if existing.resource_id != resource_id {
        return Err(AppError::NotFound(format!("resource_version {}", version_id)));
    }
    validate_transition(existing.state, VersionState::Approved)?;

    sqlx::query(
        r#"UPDATE resource_versions
              SET state = 'approved', approved_by = ?, approved_at = NOW()
            WHERE id = ?"#,
    )
    .bind(principal.user_id.to_string())
    .bind(version_id.to_string())
    .execute(pool)
    .await
    .map_err(|e| AppError::Database(format!("approve: {}", e)))?;

    audit_service::record(
        pool,
        AuditEvent {
            actor_id: Some(principal.user_id),
            actor_email: Some(&principal.email),
            action: "resource.approve",
            target_entity_type: Some("resource_version"),
            target_entity_id: Some(version_id),
            change_payload: Some(serde_json::json!({ "resource_id": resource_id })),
            ip_address: None,
            user_agent: None,
        },
    )
    .await?;
    load_version_by_id(pool, version_id).await
}

pub async fn publish_version(
    pool: &MySqlPool,
    principal: &Principal,
    resource_id: Uuid,
    version_id: Uuid,
) -> AppResult<ResourceView> {
    require(principal, Capability::ResourcePublish)?;

    let mut tx = pool
        .begin()
        .await
        .map_err(|e| AppError::Database(format!("publish tx: {}", e)))?;

    let row = sqlx::query(
        r#"SELECT state, resource_id FROM resource_versions WHERE id = ? FOR UPDATE"#,
    )
    .bind(version_id.to_string())
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| AppError::Database(format!("publish load: {}", e)))?
    .ok_or_else(|| AppError::NotFound(format!("resource_version {}", version_id)))?;

    let state_s: String = row
        .try_get("state")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let resource_id_s: String = row
        .try_get("resource_id")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let current_state = VersionState::from_db(&state_s).unwrap_or(VersionState::Draft);
    if resource_id_s != resource_id.to_string() {
        return Err(AppError::NotFound(format!("resource_version {}", version_id)));
    }
    validate_transition(current_state, VersionState::Published)?;

    sqlx::query(
        r#"UPDATE resource_versions
              SET state = 'archived'
            WHERE resource_id = ? AND state = 'published' AND id <> ?"#,
    )
    .bind(resource_id.to_string())
    .bind(version_id.to_string())
    .execute(&mut *tx)
    .await
    .map_err(|e| AppError::Database(format!("archive prior: {}", e)))?;

    sqlx::query(
        r#"UPDATE resource_versions
              SET state = 'published', published_by = ?, published_at = NOW()
            WHERE id = ?"#,
    )
    .bind(principal.user_id.to_string())
    .bind(version_id.to_string())
    .execute(&mut *tx)
    .await
    .map_err(|e| AppError::Database(format!("publish update: {}", e)))?;

    sqlx::query(
        r#"UPDATE teaching_resources
              SET current_version_id = ?, is_published = TRUE
            WHERE id = ?"#,
    )
    .bind(version_id.to_string())
    .bind(resource_id.to_string())
    .execute(&mut *tx)
    .await
    .map_err(|e| AppError::Database(format!("baseline update: {}", e)))?;

    tx.commit()
        .await
        .map_err(|e| AppError::Database(format!("publish commit: {}", e)))?;

    audit_service::record(
        pool,
        AuditEvent {
            actor_id: Some(principal.user_id),
            actor_email: Some(&principal.email),
            action: "resource.publish",
            target_entity_type: Some("resource_version"),
            target_entity_id: Some(version_id),
            change_payload: Some(serde_json::json!({ "resource_id": resource_id })),
            ip_address: None,
            user_agent: None,
        },
    )
    .await?;

    get_resource_by_id(pool, principal, resource_id).await
}

pub async fn list_resources(
    pool: &MySqlPool,
    principal: &Principal,
    limit: u32,
    offset: u32,
) -> AppResult<Vec<ResourceView>> {
    require(principal, Capability::ResourceRead)?;
    let editor = super::authorization::principal_can(principal, Capability::ResourceWrite);
    let privileged_editor = principal.is_admin() || principal.has_role(Role::Librarian);
    let limit = limit.clamp(1, 500) as i64;
    let offset = offset as i64;

    // Three-tier query strategy:
    //
    //  1. Admin / Librarian  → see every resource in any state.
    //  2. Instructor         → see published resources from anyone, plus all
    //                          resources they personally own (any state).
    //  3. Viewer / DeptHead  → published-only (current_version_id must be set
    //                          so the resource is meaningfully readable).
    //
    // The subsequent per-ID call to `get_resource_by_id` re-enforces the same
    // policy, so even if the SQL returns an unexpected row the object-level
    // check is still applied.
    let rows = if privileged_editor {
        sqlx::query(
            r#"SELECT id FROM teaching_resources ORDER BY updated_at DESC LIMIT ? OFFSET ?"#,
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(pool)
        .await
    } else if editor {
        // Instructor: own resources (any state) OR published resources from anyone.
        sqlx::query(
            r#"SELECT id FROM teaching_resources
                WHERE is_published = TRUE OR owner_id = ?
                ORDER BY updated_at DESC LIMIT ? OFFSET ?"#,
        )
        .bind(principal.user_id.to_string())
        .bind(limit)
        .bind(offset)
        .fetch_all(pool)
        .await
    } else {
        sqlx::query(
            r#"SELECT id FROM teaching_resources
                WHERE is_published = TRUE AND current_version_id IS NOT NULL
                ORDER BY updated_at DESC LIMIT ? OFFSET ?"#,
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(pool)
        .await
    }
    .map_err(|e| AppError::Database(format!("list_resources: {}", e)))?;

    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let id: String = row
            .try_get("id")
            .map_err(|e| AppError::Database(e.to_string()))?;
        let rid = Uuid::parse_str(&id).map_err(|e| AppError::Database(e.to_string()))?;
        out.push(get_resource_by_id(pool, principal, rid).await?);
    }
    Ok(out)
}

pub async fn get_resource_by_id(
    pool: &MySqlPool,
    principal: &Principal,
    resource_id: Uuid,
) -> AppResult<ResourceView> {
    require(principal, Capability::ResourceRead)?;

    let row = sqlx::query(
        r#"SELECT id, title, owner_id, resource_type, is_published,
                  current_version_id, latest_version_id, created_at, updated_at
             FROM teaching_resources
            WHERE id = ?"#,
    )
    .bind(resource_id.to_string())
    .fetch_optional(pool)
    .await
    .map_err(|e| AppError::Database(format!("get_resource: {}", e)))?
    .ok_or_else(|| AppError::NotFound(format!("teaching_resource {}", resource_id)))?;

    let title: String = row
        .try_get("title")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let owner_id: Option<String> = row
        .try_get("owner_id")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let resource_type_s: String = row
        .try_get("resource_type")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let is_published: bool = row
        .try_get::<i8, _>("is_published")
        .map(|b| b != 0)
        .map_err(|e| AppError::Database(e.to_string()))?;
    let current_version_id: Option<String> = row
        .try_get("current_version_id")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let latest_version_id: Option<String> = row
        .try_get("latest_version_id")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let created_at: NaiveDateTime = row
        .try_get("created_at")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let updated_at: NaiveDateTime = row
        .try_get("updated_at")
        .map_err(|e| AppError::Database(e.to_string()))?;

    let editor = super::authorization::principal_can(principal, Capability::ResourceWrite);

    // Object-level visibility for Instructors.
    //
    // Instructors hold `ResourceWrite` (editor == true) but their write access
    // is already scoped to resources they own (see `create_draft_version`).
    // Their read access must mirror the same boundary: they may read any
    // published resource, and may read their own unpublished resources, but
    // must not be able to discover or read drafts belonging to other
    // Instructors — even when the resource UUID is known out-of-band.
    //
    // Admin and Librarian are unrestricted (fall through).
    // Non-editors (Viewer, DeptHead) are gated separately below by the
    // `effective_version.is_none()` check.
    //
    // We surface NotFound rather than Forbidden so the response does not
    // reveal whether the resource exists at all.
    if editor && !principal.is_admin() && !principal.has_role(Role::Librarian) {
        let owner_uuid = owner_id.as_deref().and_then(|s| Uuid::parse_str(s).ok());
        if !is_published && owner_uuid != Some(principal.user_id) {
            return Err(AppError::NotFound(format!("teaching_resource {}", resource_id)));
        }
    }

    let effective_version_id = if editor {
        latest_version_id.clone().or_else(|| current_version_id.clone())
    } else {
        current_version_id.clone()
    };

    let effective_version = match effective_version_id {
        Some(ref s) => {
            let id = Uuid::parse_str(s).map_err(|e| AppError::Database(e.to_string()))?;
            Some(load_version_by_id(pool, id).await?)
        }
        None => None,
    };

    if !editor && effective_version.is_none() {
        return Err(AppError::NotFound(format!("teaching_resource {}", resource_id)));
    }

    Ok(ResourceView {
        id: resource_id,
        title,
        resource_type: ResourceType::from_db(&resource_type_s).unwrap_or(ResourceType::Other),
        owner_id: owner_id.as_deref().and_then(|s| Uuid::parse_str(s).ok()),
        is_published,
        current_version_id: current_version_id
            .as_deref()
            .and_then(|s| Uuid::parse_str(s).ok()),
        latest_version_id: latest_version_id
            .as_deref()
            .and_then(|s| Uuid::parse_str(s).ok()),
        created_at,
        updated_at,
        effective_version,
    })
}

pub async fn list_versions(
    pool: &MySqlPool,
    principal: &Principal,
    resource_id: Uuid,
) -> AppResult<Vec<ResourceVersionView>> {
    require(principal, Capability::ResourceWrite)?;
    // Verify the caller has object-level visibility of the parent resource
    // before exposing the full version history.  This prevents an Instructor
    // from enumerating draft versions of resources they do not own.
    get_resource_by_id(pool, principal, resource_id).await?;
    let rows = sqlx::query(
        r#"SELECT id FROM resource_versions
            WHERE resource_id = ?
            ORDER BY version_number DESC"#,
    )
    .bind(resource_id.to_string())
    .fetch_all(pool)
    .await
    .map_err(|e| AppError::Database(format!("list_versions: {}", e)))?;

    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let id: String = row
            .try_get("id")
            .map_err(|e| AppError::Database(e.to_string()))?;
        let vid = Uuid::parse_str(&id).map_err(|e| AppError::Database(e.to_string()))?;
        out.push(load_version_by_id(pool, vid).await?);
    }
    Ok(out)
}

pub async fn get_version(
    pool: &MySqlPool,
    principal: &Principal,
    resource_id: Uuid,
    version_id: Uuid,
) -> AppResult<ResourceVersionView> {
    require(principal, Capability::ResourceRead)?;
    // Verify the caller has object-level visibility of the parent resource
    // before exposing any specific version.  An Instructor who cannot see
    // a resource (non-owned draft) must also not be able to fetch its
    // individual versions by UUID.
    get_resource_by_id(pool, principal, resource_id).await?;
    let version = load_version_by_id(pool, version_id).await?;
    if version.resource_id != resource_id {
        return Err(AppError::NotFound(format!("resource_version {}", version_id)));
    }
    let editor = super::authorization::principal_can(principal, Capability::ResourceWrite);
    if !editor && version.state != VersionState::Published {
        return Err(AppError::Forbidden);
    }
    Ok(version)
}

// ---------------------------------------------------------------------------
// Internal loader
// ---------------------------------------------------------------------------

async fn load_version_by_id(
    pool: &MySqlPool,
    version_id: Uuid,
) -> AppResult<ResourceVersionView> {
    let row = sqlx::query(
        r#"SELECT id, resource_id, version_number, content_url, mime_type, size_bytes,
                  description, change_summary, state, created_by, created_at,
                  approved_by, approved_at, published_by, published_at
             FROM resource_versions
            WHERE id = ?"#,
    )
    .bind(version_id.to_string())
    .fetch_optional(pool)
    .await
    .map_err(|e| AppError::Database(format!("load_version: {}", e)))?
    .ok_or_else(|| AppError::NotFound(format!("resource_version {}", version_id)))?;

    let id: String = row.try_get("id").map_err(|e| AppError::Database(e.to_string()))?;
    let resource_id: String = row
        .try_get("resource_id")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let version_number: i32 = row
        .try_get("version_number")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let content_url: Option<String> = row
        .try_get("content_url")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let mime_type: Option<String> = row
        .try_get("mime_type")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let size_bytes: Option<i64> = row
        .try_get("size_bytes")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let description: Option<String> = row
        .try_get("description")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let change_summary: Option<String> = row
        .try_get("change_summary")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let state_s: String = row
        .try_get("state")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let created_by: Option<String> = row
        .try_get("created_by")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let created_at: NaiveDateTime = row
        .try_get("created_at")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let approved_by: Option<String> = row
        .try_get("approved_by")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let approved_at: Option<NaiveDateTime> = row
        .try_get("approved_at")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let published_by: Option<String> = row
        .try_get("published_by")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let published_at: Option<NaiveDateTime> = row
        .try_get("published_at")
        .map_err(|e| AppError::Database(e.to_string()))?;

    Ok(ResourceVersionView {
        id: Uuid::parse_str(&id).map_err(|e| AppError::Database(e.to_string()))?,
        resource_id: Uuid::parse_str(&resource_id)
            .map_err(|e| AppError::Database(e.to_string()))?,
        version_number,
        content_url,
        mime_type,
        size_bytes,
        description,
        change_summary,
        state: VersionState::from_db(&state_s).unwrap_or(VersionState::Draft),
        created_by: created_by.as_deref().and_then(|s| Uuid::parse_str(s).ok()),
        created_at,
        approved_by: approved_by.as_deref().and_then(|s| Uuid::parse_str(s).ok()),
        approved_at,
        published_by: published_by.as_deref().and_then(|s| Uuid::parse_str(s).ok()),
        published_at,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resource_type_db_round_trip() {
        for t in [
            ResourceType::Document,
            ResourceType::Video,
            ResourceType::Presentation,
            ResourceType::Assessment,
            ResourceType::ExternalLink,
            ResourceType::Dataset,
            ResourceType::Other,
        ] {
            assert_eq!(ResourceType::from_db(t.as_db()), Some(t));
        }
        assert!(ResourceType::from_db("nope").is_none());
    }

    #[test]
    fn validate_title_edges() {
        assert!(validate_title("ab").is_err());
        assert!(validate_title("abc").is_ok());
        assert!(validate_title(&"x".repeat(501)).is_err());
    }

    // -----------------------------------------------------------------
    // Phase 3 additional coverage
    // -----------------------------------------------------------------

    #[test]
    fn validate_title_boundary_cases() {
        // 2 chars -> below TITLE_MIN.
        assert!(validate_title("ab").is_err());
        // 3 chars -> lower bound.
        assert!(validate_title("abc").is_ok());
        // 500 chars -> upper bound, accepted.
        assert!(validate_title(&"x".repeat(500)).is_ok());
        // 501 chars -> one too long.
        assert!(validate_title(&"x".repeat(501)).is_err());
    }

    #[test]
    fn resource_type_external_link_round_trip() {
        assert_eq!(
            ResourceType::from_db("external_link"),
            Some(ResourceType::ExternalLink)
        );
        assert_eq!(ResourceType::ExternalLink.as_db(), "external_link");
    }
}
