//! Journal CRUD + versioning workflow.
//!
//! # Data model at a glance
//!
//! ```text
//! journals
//!   id, title, author_id, abstract_text,
//!   is_published,                -- mirrors the existence of a published version
//!   current_version_id,          -- the PUBLISHED operational baseline (may be NULL)
//!   latest_version_id            -- the newest row in journal_versions (any state)
//!
//! journal_versions
//!   id, journal_id, version_number,
//!   title, body, change_summary, state,
//!   created_by, created_at,
//!   approved_by, approved_at,
//!   published_by, published_at
//! ```
//!
//! # Workflow
//!
//! 1. `create_journal` writes the master row plus version #1 in `draft`.
//! 2. `create_draft_version` stamps a new version number on the next edit.
//! 3. `approve_version` moves a draft to `approved`.
//! 4. `publish_version` moves an approved version to `published`, sets
//!    `journals.current_version_id`, and archives the previously-published
//!    version (if any) to preserve the invariant "at most one published
//!    version per journal".
//!
//! Everything runs inside explicit database transactions so a crash
//! mid-transition cannot leave `journals` pointing at a version that
//! isn't marked `published`.

use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use sqlx::{MySqlPool, Row};
use uuid::Uuid;

use super::audit_service::{self, AuditEvent};
use super::authorization::{require, Capability};
use super::principal::Principal;
use crate::domain::versioning::{validate_transition, VersionState};
use crate::errors::{AppError, AppResult};

// ---------------------------------------------------------------------------
// Inputs and view models
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct JournalCreateInput {
    pub title: String,
    pub abstract_text: Option<String>,
    pub body: String,
    pub change_summary: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct JournalEditInput {
    pub title: Option<String>,
    pub body: String,
    pub change_summary: Option<String>,
}

/// A thin projection joining a `journals` row with its currently-selected
/// version (baseline for read-only callers, latest draft for editors).
#[derive(Debug, Clone, Serialize)]
pub struct JournalView {
    pub id: Uuid,
    pub title: String,
    pub abstract_text: Option<String>,
    pub author_id: Option<Uuid>,
    pub is_published: bool,
    pub current_version_id: Option<Uuid>,
    pub latest_version_id: Option<Uuid>,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
    /// The version that was selected for this view — either the published
    /// baseline (for non-editors) or the latest draft (for editors / when
    /// a baseline doesn't exist yet).
    pub effective_version: Option<JournalVersionView>,
}

#[derive(Debug, Clone, Serialize)]
pub struct JournalVersionView {
    pub id: Uuid,
    pub journal_id: Uuid,
    pub version_number: i32,
    pub title: Option<String>,
    pub body: Option<String>,
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
const BODY_MAX: usize = 500_000;
const CHANGE_SUMMARY_MAX: usize = 2_000;
const ABSTRACT_MAX: usize = 4_000;

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

fn validate_body(body: &str) -> AppResult<()> {
    if body.trim().is_empty() {
        return Err(AppError::Validation("body must not be empty".into()));
    }
    if body.chars().count() > BODY_MAX {
        return Err(AppError::Validation(format!(
            "body exceeds {} character limit",
            BODY_MAX
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

/// Create a new journal plus its initial version #1 in `draft` state.
pub async fn create_journal(
    pool: &MySqlPool,
    principal: &Principal,
    input: JournalCreateInput,
) -> AppResult<JournalView> {
    require(principal, Capability::JournalWrite)?;
    validate_title(&input.title)?;
    validate_body(&input.body)?;
    validate_optional("abstract_text", input.abstract_text.as_deref(), ABSTRACT_MAX)?;
    validate_optional(
        "change_summary",
        input.change_summary.as_deref(),
        CHANGE_SUMMARY_MAX,
    )?;

    let mut tx = pool
        .begin()
        .await
        .map_err(|e| AppError::Database(format!("create_journal tx begin: {}", e)))?;

    let journal_id = Uuid::new_v4();
    let version_id = Uuid::new_v4();

    sqlx::query(
        r#"INSERT INTO journals (id, title, author_id, abstract_text, is_published)
           VALUES (?, ?, ?, ?, FALSE)"#,
    )
    .bind(journal_id.to_string())
    .bind(input.title.trim())
    .bind(principal.user_id.to_string())
    .bind(input.abstract_text.as_deref())
    .execute(&mut *tx)
    .await
    .map_err(|e| AppError::Database(format!("create_journal insert: {}", e)))?;

    sqlx::query(
        r#"INSERT INTO journal_versions
           (id, journal_id, version_number, title, body, change_summary, state, created_by)
           VALUES (?, ?, 1, ?, ?, ?, 'draft', ?)"#,
    )
    .bind(version_id.to_string())
    .bind(journal_id.to_string())
    .bind(input.title.trim())
    .bind(&input.body)
    .bind(input.change_summary.as_deref())
    .bind(principal.user_id.to_string())
    .execute(&mut *tx)
    .await
    .map_err(|e| AppError::Database(format!("create_journal version insert: {}", e)))?;

    sqlx::query("UPDATE journals SET latest_version_id = ? WHERE id = ?")
        .bind(version_id.to_string())
        .bind(journal_id.to_string())
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::Database(format!("create_journal latest set: {}", e)))?;

    tx.commit()
        .await
        .map_err(|e| AppError::Database(format!("create_journal tx commit: {}", e)))?;

    audit_service::record(
        pool,
        AuditEvent {
            actor_id: Some(principal.user_id),
            actor_email: Some(&principal.email),
            action: "journal.create",
            target_entity_type: Some("journal"),
            target_entity_id: Some(journal_id),
            change_payload: Some(serde_json::json!({
                "version_id": version_id,
                "title": input.title.trim(),
            })),
            ip_address: None,
            user_agent: None,
        },
    )
    .await?;

    get_journal_by_id(pool, principal, journal_id).await
}

/// Append a new draft version to an existing journal.
pub async fn create_draft_version(
    pool: &MySqlPool,
    principal: &Principal,
    journal_id: Uuid,
    input: JournalEditInput,
) -> AppResult<JournalVersionView> {
    require(principal, Capability::JournalWrite)?;
    validate_body(&input.body)?;
    if let Some(ref t) = input.title {
        validate_title(t)?;
    }
    validate_optional(
        "change_summary",
        input.change_summary.as_deref(),
        CHANGE_SUMMARY_MAX,
    )?;

    let mut tx = pool
        .begin()
        .await
        .map_err(|e| AppError::Database(format!("create_draft_version tx: {}", e)))?;

    // Verify the parent exists and grab the current max version_number.
    let max_row = sqlx::query(
        r#"SELECT COALESCE(MAX(version_number), 0) AS max_ver
             FROM journal_versions
            WHERE journal_id = ?
            FOR UPDATE"#,
    )
    .bind(journal_id.to_string())
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| AppError::Database(format!("max ver: {}", e)))?;
    let max_ver: i32 = max_row
        .try_get("max_ver")
        .map_err(|e| AppError::Database(e.to_string()))?;
    if max_ver == 0 {
        return Err(AppError::NotFound(format!("journal {}", journal_id)));
    }

    let new_ver = max_ver + 1;
    let version_id = Uuid::new_v4();

    sqlx::query(
        r#"INSERT INTO journal_versions
           (id, journal_id, version_number, title, body, change_summary, state, created_by)
           VALUES (?, ?, ?, ?, ?, ?, 'draft', ?)"#,
    )
    .bind(version_id.to_string())
    .bind(journal_id.to_string())
    .bind(new_ver)
    .bind(input.title.as_deref())
    .bind(&input.body)
    .bind(input.change_summary.as_deref())
    .bind(principal.user_id.to_string())
    .execute(&mut *tx)
    .await
    .map_err(|e| AppError::Database(format!("insert draft: {}", e)))?;

    sqlx::query("UPDATE journals SET latest_version_id = ? WHERE id = ?")
        .bind(version_id.to_string())
        .bind(journal_id.to_string())
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::Database(format!("update latest: {}", e)))?;

    tx.commit()
        .await
        .map_err(|e| AppError::Database(format!("create_draft_version commit: {}", e)))?;

    audit_service::record(
        pool,
        AuditEvent {
            actor_id: Some(principal.user_id),
            actor_email: Some(&principal.email),
            action: "journal.draft.create",
            target_entity_type: Some("journal_version"),
            target_entity_id: Some(version_id),
            change_payload: Some(serde_json::json!({
                "journal_id": journal_id,
                "version_number": new_ver,
            })),
            ip_address: None,
            user_agent: None,
        },
    )
    .await?;

    load_version_by_id(pool, version_id).await
}

/// Approve a draft version.
pub async fn approve_version(
    pool: &MySqlPool,
    principal: &Principal,
    journal_id: Uuid,
    version_id: Uuid,
) -> AppResult<JournalVersionView> {
    require(principal, Capability::JournalApprove)?;
    let existing = load_version_by_id(pool, version_id).await?;
    if existing.journal_id != journal_id {
        return Err(AppError::NotFound(format!("journal_version {}", version_id)));
    }
    validate_transition(existing.state, VersionState::Approved)?;

    sqlx::query(
        r#"UPDATE journal_versions
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
            action: "journal.approve",
            target_entity_type: Some("journal_version"),
            target_entity_id: Some(version_id),
            change_payload: Some(serde_json::json!({ "journal_id": journal_id })),
            ip_address: None,
            user_agent: None,
        },
    )
    .await?;
    load_version_by_id(pool, version_id).await
}

/// Publish an approved version — atomically archives any previously-published
/// version and points `journals.current_version_id` at the new baseline.
pub async fn publish_version(
    pool: &MySqlPool,
    principal: &Principal,
    journal_id: Uuid,
    version_id: Uuid,
) -> AppResult<JournalView> {
    require(principal, Capability::JournalPublish)?;

    let mut tx = pool
        .begin()
        .await
        .map_err(|e| AppError::Database(format!("publish tx: {}", e)))?;

    let row = sqlx::query(
        r#"SELECT state, journal_id FROM journal_versions WHERE id = ? FOR UPDATE"#,
    )
    .bind(version_id.to_string())
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| AppError::Database(format!("publish load: {}", e)))?
    .ok_or_else(|| AppError::NotFound(format!("journal_version {}", version_id)))?;

    let state_s: String = row
        .try_get("state")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let journal_id_s: String = row
        .try_get("journal_id")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let current_state = VersionState::from_db(&state_s).unwrap_or(VersionState::Draft);
    if journal_id_s != journal_id.to_string() {
        return Err(AppError::NotFound(format!("journal_version {}", version_id)));
    }
    validate_transition(current_state, VersionState::Published)?;

    sqlx::query(
        r#"UPDATE journal_versions
              SET state = 'archived'
            WHERE journal_id = ? AND state = 'published' AND id <> ?"#,
    )
    .bind(journal_id.to_string())
    .bind(version_id.to_string())
    .execute(&mut *tx)
    .await
    .map_err(|e| AppError::Database(format!("archive prior: {}", e)))?;

    sqlx::query(
        r#"UPDATE journal_versions
              SET state = 'published', published_by = ?, published_at = NOW()
            WHERE id = ?"#,
    )
    .bind(principal.user_id.to_string())
    .bind(version_id.to_string())
    .execute(&mut *tx)
    .await
    .map_err(|e| AppError::Database(format!("publish update: {}", e)))?;

    sqlx::query(
        r#"UPDATE journals
              SET current_version_id = ?, is_published = TRUE
            WHERE id = ?"#,
    )
    .bind(version_id.to_string())
    .bind(journal_id.to_string())
    .execute(&mut *tx)
    .await
    .map_err(|e| AppError::Database(format!("journal baseline update: {}", e)))?;

    tx.commit()
        .await
        .map_err(|e| AppError::Database(format!("publish commit: {}", e)))?;

    audit_service::record(
        pool,
        AuditEvent {
            actor_id: Some(principal.user_id),
            actor_email: Some(&principal.email),
            action: "journal.publish",
            target_entity_type: Some("journal_version"),
            target_entity_id: Some(version_id),
            change_payload: Some(serde_json::json!({ "journal_id": journal_id })),
            ip_address: None,
            user_agent: None,
        },
    )
    .await?;

    get_journal_by_id(pool, principal, journal_id).await
}

/// List journals visible to the principal.
///
/// * Editors (`JournalWrite`) see every row.
/// * Readers (`JournalRead` only) see rows that have a published baseline.
pub async fn list_journals(
    pool: &MySqlPool,
    principal: &Principal,
    limit: u32,
    offset: u32,
) -> AppResult<Vec<JournalView>> {
    require(principal, Capability::JournalRead)?;
    let editor = super::authorization::principal_can(principal, Capability::JournalWrite);
    let limit = limit.clamp(1, 500) as i64;
    let offset = offset as i64;

    let rows = if editor {
        sqlx::query(
            r#"SELECT id FROM journals ORDER BY updated_at DESC LIMIT ? OFFSET ?"#,
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(pool)
        .await
    } else {
        sqlx::query(
            r#"SELECT id FROM journals
                WHERE is_published = TRUE AND current_version_id IS NOT NULL
                ORDER BY updated_at DESC LIMIT ? OFFSET ?"#,
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(pool)
        .await
    }
    .map_err(|e| AppError::Database(format!("list_journals: {}", e)))?;

    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let id: String = row
            .try_get("id")
            .map_err(|e| AppError::Database(e.to_string()))?;
        let jid = Uuid::parse_str(&id).map_err(|e| AppError::Database(e.to_string()))?;
        out.push(get_journal_by_id(pool, principal, jid).await?);
    }
    Ok(out)
}

/// Load a journal and pick the correct "effective version" for the caller.
pub async fn get_journal_by_id(
    pool: &MySqlPool,
    principal: &Principal,
    journal_id: Uuid,
) -> AppResult<JournalView> {
    require(principal, Capability::JournalRead)?;

    let row = sqlx::query(
        r#"SELECT id, title, author_id, abstract_text, is_published,
                  current_version_id, latest_version_id, created_at, updated_at
             FROM journals
            WHERE id = ?"#,
    )
    .bind(journal_id.to_string())
    .fetch_optional(pool)
    .await
    .map_err(|e| AppError::Database(format!("get_journal: {}", e)))?
    .ok_or_else(|| AppError::NotFound(format!("journal {}", journal_id)))?;

    let title: String = row
        .try_get("title")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let author_id: Option<String> = row
        .try_get("author_id")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let abstract_text: Option<String> = row
        .try_get("abstract_text")
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

    let editor = super::authorization::principal_can(principal, Capability::JournalWrite);
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

    // Readers without a published baseline see 404.
    if !editor && effective_version.is_none() {
        return Err(AppError::NotFound(format!("journal {}", journal_id)));
    }

    Ok(JournalView {
        id: journal_id,
        title,
        abstract_text,
        author_id: author_id.as_deref().and_then(|s| Uuid::parse_str(s).ok()),
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

/// List every version of a journal (newest first). Editors only.
pub async fn list_versions(
    pool: &MySqlPool,
    principal: &Principal,
    journal_id: Uuid,
) -> AppResult<Vec<JournalVersionView>> {
    require(principal, Capability::JournalWrite)?;
    let rows = sqlx::query(
        r#"SELECT id FROM journal_versions
            WHERE journal_id = ?
            ORDER BY version_number DESC"#,
    )
    .bind(journal_id.to_string())
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

/// Fetch one specific version. Non-editors can only read the published one.
pub async fn get_version(
    pool: &MySqlPool,
    principal: &Principal,
    journal_id: Uuid,
    version_id: Uuid,
) -> AppResult<JournalVersionView> {
    require(principal, Capability::JournalRead)?;
    let version = load_version_by_id(pool, version_id).await?;
    if version.journal_id != journal_id {
        return Err(AppError::NotFound(format!("journal_version {}", version_id)));
    }
    let editor = super::authorization::principal_can(principal, Capability::JournalWrite);
    if !editor && version.state != VersionState::Published {
        return Err(AppError::Forbidden);
    }
    Ok(version)
}

// ---------------------------------------------------------------------------
// Internal loaders
// ---------------------------------------------------------------------------

async fn load_version_by_id(
    pool: &MySqlPool,
    version_id: Uuid,
) -> AppResult<JournalVersionView> {
    let row = sqlx::query(
        r#"SELECT id, journal_id, version_number, title, body, change_summary,
                  state, created_by, created_at,
                  approved_by, approved_at, published_by, published_at
             FROM journal_versions
            WHERE id = ?"#,
    )
    .bind(version_id.to_string())
    .fetch_optional(pool)
    .await
    .map_err(|e| AppError::Database(format!("load_version: {}", e)))?
    .ok_or_else(|| AppError::NotFound(format!("journal_version {}", version_id)))?;

    let id: String = row.try_get("id").map_err(|e| AppError::Database(e.to_string()))?;
    let journal_id: String = row
        .try_get("journal_id")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let version_number: i32 = row
        .try_get("version_number")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let title: Option<String> = row
        .try_get("title")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let body: Option<String> = row
        .try_get("body")
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

    Ok(JournalVersionView {
        id: Uuid::parse_str(&id).map_err(|e| AppError::Database(e.to_string()))?,
        journal_id: Uuid::parse_str(&journal_id)
            .map_err(|e| AppError::Database(e.to_string()))?,
        version_number,
        title,
        body,
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
    fn validate_title_rules() {
        assert!(validate_title("ok").is_err()); // 2 chars
        assert!(validate_title("long enough").is_ok());
        assert!(validate_title(&"x".repeat(501)).is_err());
    }

    #[test]
    fn validate_body_rules() {
        assert!(validate_body("").is_err());
        assert!(validate_body("   ").is_err());
        assert!(validate_body("some real body").is_ok());
    }

    #[test]
    fn validate_optional_string_length() {
        assert!(validate_optional("x", None, 10).is_ok());
        assert!(validate_optional("x", Some("short"), 10).is_ok());
        assert!(validate_optional("x", Some("way too long for this field"), 5).is_err());
    }

    // -----------------------------------------------------------------
    // Phase 3 additional coverage — title/body/optional edges
    // -----------------------------------------------------------------

    #[test]
    fn validate_title_boundary_cases() {
        // 2 characters is one short of TITLE_MIN=3 -> rejected.
        assert!(validate_title("ab").is_err());
        // 3 characters is exactly the minimum -> accepted.
        assert!(validate_title("abc").is_ok());
        // Exactly 500 characters is the maximum -> accepted.
        let exact = "x".repeat(500);
        assert!(validate_title(&exact).is_ok());
        // 501 characters is one over -> rejected.
        let over = "x".repeat(501);
        assert!(validate_title(&over).is_err());
    }

    #[test]
    fn validate_body_rejects_whitespace_only() {
        // Tabs/newlines are trimmed to empty — reject.
        assert!(validate_body("\t\t\n  \n").is_err());
        assert!(validate_body("").is_err());
        // Any real content is fine.
        assert!(validate_body("hello").is_ok());
    }

    #[test]
    fn validate_optional_respects_limit_exactly() {
        // At the limit is still OK.
        let s = "x".repeat(10);
        assert!(validate_optional("field", Some(&s), 10).is_ok());
        // One over the limit fails.
        let s = "x".repeat(11);
        assert!(validate_optional("field", Some(&s), 10).is_err());
        // None is always OK regardless of limit.
        assert!(validate_optional("field", None, 0).is_ok());
    }
}
