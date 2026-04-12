//! Section CRUD + versioning workflow.
//!
//! Sections belong to a course and carry per-term scheduling data:
//! section_code, term, year, capacity, assigned instructor, location,
//! schedule, and notes. Version history follows the same state machine
//! (draft → approved → published → archived) as journals, resources,
//! and courses.
//!
//! # Authorization layers
//!
//! * `Capability::SectionWrite` gates edits at the role layer.
//! * `ensure_section_in_scope` additionally verifies that:
//!   - Admin / DepartmentHead can edit any section in their department.
//!   - Instructors can only edit sections where they are the assigned
//!     `instructor_id`.
//! * `Capability::SectionApprove` / `SectionPublish` further narrow
//!   approval/publish to DepartmentHead + Admin per the RBAC matrix.

use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use sqlx::{MySqlPool, Row};
use uuid::Uuid;

use super::audit_service::{self, AuditEvent};
use super::authorization::{principal_can, require, Capability};
use super::encryption::FieldEncryption;
use super::principal::{Principal, Role};
use crate::domain::versioning::{validate_transition, VersionState};
use crate::errors::{AppError, AppResult};

// ---------------------------------------------------------------------------
// Inputs and view models
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct SectionCreateInput {
    pub course_id: Uuid,
    pub section_code: String,
    pub term: String, // "fall" | "spring" | "summer" | "winter"
    pub year: i32,
    pub capacity: i32,
    pub instructor_id: Option<Uuid>,
    pub location: Option<String>,
    pub schedule_note: Option<String>,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SectionEditInput {
    pub instructor_id: Option<Uuid>,
    pub capacity: Option<i32>,
    pub location: Option<String>,
    pub schedule_note: Option<String>,
    pub notes: Option<String>,
    pub change_summary: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SectionView {
    pub id: Uuid,
    pub course_id: Uuid,
    pub course_code: String,
    pub department_id: Option<Uuid>,
    pub section_code: String,
    pub term: String,
    pub year: i32,
    pub capacity: Option<i32>,
    pub instructor_id: Option<Uuid>,
    pub is_active: bool,
    pub current_version_id: Option<Uuid>,
    pub latest_version_id: Option<Uuid>,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
    pub effective_version: Option<SectionVersionView>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SectionVersionView {
    pub id: Uuid,
    pub section_id: Uuid,
    pub version_number: i32,
    pub location: Option<String>,
    pub schedule_note: Option<String>,
    pub notes: Option<String>,
    pub state: VersionState,
    pub created_by: Option<Uuid>,
    pub created_at: NaiveDateTime,
    pub approved_at: Option<NaiveDateTime>,
    pub published_at: Option<NaiveDateTime>,
}

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

const SECTION_CODE_MAX: usize = 20;
const LOCATION_MAX: usize = 255;
const SCHEDULE_MAX: usize = 500;
const NOTES_MAX: usize = 2_000;
const CAPACITY_MIN: i32 = 1;
const CAPACITY_MAX: i32 = 1_000;
const YEAR_MIN: i32 = 2000;
const YEAR_MAX: i32 = 2100;

pub const VALID_TERMS: &[&str] = &["fall", "spring", "summer", "winter"];

pub fn is_valid_term(term: &str) -> bool {
    VALID_TERMS.iter().any(|t| t.eq_ignore_ascii_case(term))
}

pub fn normalize_term(term: &str) -> String {
    term.trim().to_lowercase()
}

pub fn is_valid_section_code(code: &str) -> bool {
    let t = code.trim();
    if t.is_empty() || t.len() > SECTION_CODE_MAX {
        return false;
    }
    t.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

pub fn validate_capacity(cap: i32) -> AppResult<()> {
    if !(CAPACITY_MIN..=CAPACITY_MAX).contains(&cap) {
        return Err(AppError::Validation(format!(
            "capacity must be between {} and {}",
            CAPACITY_MIN, CAPACITY_MAX
        )));
    }
    Ok(())
}

pub fn validate_year(year: i32) -> AppResult<()> {
    if !(YEAR_MIN..=YEAR_MAX).contains(&year) {
        return Err(AppError::Validation(format!(
            "year must be between {} and {}",
            YEAR_MIN, YEAR_MAX
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

fn validate_section_inputs(
    section_code: &str,
    term: &str,
    year: i32,
    capacity: i32,
    location: Option<&str>,
    schedule: Option<&str>,
    notes: Option<&str>,
) -> AppResult<()> {
    if !is_valid_section_code(section_code) {
        return Err(AppError::Validation(format!(
            "section_code '{}' is invalid (alphanumeric + -/_, max {})",
            section_code, SECTION_CODE_MAX
        )));
    }
    if !is_valid_term(term) {
        return Err(AppError::Validation(format!(
            "term must be one of: {}",
            VALID_TERMS.join(", ")
        )));
    }
    validate_year(year)?;
    validate_capacity(capacity)?;
    validate_optional("location", location, LOCATION_MAX)?;
    validate_optional("schedule_note", schedule, SCHEDULE_MAX)?;
    validate_optional("notes", notes, NOTES_MAX)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

pub async fn create_section(
    pool: &MySqlPool,
    principal: &Principal,
    input: SectionCreateInput,
    enc: &FieldEncryption,
) -> AppResult<SectionView> {
    require(principal, Capability::SectionWrite)?;
    validate_section_inputs(
        &input.section_code,
        &input.term,
        input.year,
        input.capacity,
        input.location.as_deref(),
        input.schedule_note.as_deref(),
        input.notes.as_deref(),
    )?;

    // The course must exist and be in scope.
    let (course_code, course_dept) = load_course_meta(pool, input.course_id).await?;
    ensure_dept_scope(principal, course_dept)?;

    // Instructor-only scope: an Instructor without admin/dept-head role
    // must assign themself (or no instructor at all).
    if principal.has_role(Role::Instructor)
        && !principal.is_admin()
        && !principal.has_role(Role::DepartmentHead)
    {
        if let Some(assigned) = input.instructor_id {
            if assigned != principal.user_id {
                return Err(AppError::Forbidden);
            }
        }
    }

    // Instructor user, if any, must exist.
    if let Some(instr) = input.instructor_id {
        let exists = sqlx::query("SELECT 1 FROM users WHERE id = ? LIMIT 1")
            .bind(instr.to_string())
            .fetch_optional(pool)
            .await
            .map_err(|e| AppError::Database(format!("instructor lookup: {}", e)))?;
        if exists.is_none() {
            return Err(AppError::Validation(format!(
                "instructor {} does not exist",
                instr
            )));
        }
    }

    // Duplicate check: (course_id, section_code, term, year) is unique.
    let dup = sqlx::query(
        r#"SELECT 1 FROM sections
            WHERE course_id = ? AND section_code = ? AND term = ? AND year = ?
            LIMIT 1"#,
    )
    .bind(input.course_id.to_string())
    .bind(input.section_code.trim())
    .bind(normalize_term(&input.term))
    .bind(input.year)
    .fetch_optional(pool)
    .await
    .map_err(|e| AppError::Database(format!("section dup check: {}", e)))?;
    if dup.is_some() {
        return Err(AppError::Conflict(format!(
            "section {} already exists for {} {} {}",
            input.section_code.trim(),
            course_code,
            input.term,
            input.year
        )));
    }

    let mut tx = pool
        .begin()
        .await
        .map_err(|e| AppError::Database(format!("create_section tx: {}", e)))?;

    let section_id = Uuid::new_v4();
    let version_id = Uuid::new_v4();

    sqlx::query(
        r#"INSERT INTO sections
           (id, course_id, instructor_id, section_code, term, year, capacity, is_active)
           VALUES (?, ?, ?, ?, ?, ?, ?, TRUE)"#,
    )
    .bind(section_id.to_string())
    .bind(input.course_id.to_string())
    .bind(input.instructor_id.map(|u| u.to_string()))
    .bind(input.section_code.trim())
    .bind(normalize_term(&input.term))
    .bind(input.year)
    .bind(input.capacity)
    .execute(&mut *tx)
    .await
    .map_err(|e| AppError::Database(format!("create section insert: {}", e)))?;

    let (encrypted_notes, notes_flag): (Option<String>, i8) = match enc.encrypt_opt(input.notes.as_deref())? {
        Some(ref v) if FieldEncryption::is_encrypted(v) => (Some(v.clone()), 1),
        other => (other, 0),
    };

    sqlx::query(
        r#"INSERT INTO section_versions
           (id, section_id, version_number, location, schedule_json, notes, notes_encrypted, state, created_by)
           VALUES (?, ?, 1, ?, CAST(? AS JSON), ?, ?, 'draft', ?)"#,
    )
    .bind(version_id.to_string())
    .bind(section_id.to_string())
    .bind(input.location.as_deref())
    .bind(
        input
            .schedule_note
            .as_deref()
            .map(|s| serde_json::json!({ "note": s }).to_string())
            .unwrap_or_else(|| "null".to_string()),
    )
    .bind(encrypted_notes.as_deref())
    .bind(notes_flag)
    .bind(principal.user_id.to_string())
    .execute(&mut *tx)
    .await
    .map_err(|e| AppError::Database(format!("create section_version insert: {}", e)))?;

    sqlx::query("UPDATE sections SET latest_version_id = ? WHERE id = ?")
        .bind(version_id.to_string())
        .bind(section_id.to_string())
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::Database(format!("update latest: {}", e)))?;

    tx.commit()
        .await
        .map_err(|e| AppError::Database(format!("create_section commit: {}", e)))?;

    audit_service::record(
        pool,
        AuditEvent {
            actor_id: Some(principal.user_id),
            actor_email: Some(&principal.email),
            action: "section.create",
            target_entity_type: Some("section"),
            target_entity_id: Some(section_id),
            change_payload: Some(serde_json::json!({
                "course_id": input.course_id,
                "section_code": input.section_code.trim(),
                "term": normalize_term(&input.term),
                "year": input.year,
                "capacity": input.capacity,
            })),
            ip_address: None,
            user_agent: None,
        },
    )
    .await?;

    get_section_by_id(pool, principal, section_id, enc).await
}

pub async fn create_draft_version(
    pool: &MySqlPool,
    principal: &Principal,
    section_id: Uuid,
    input: SectionEditInput,
    enc: &FieldEncryption,
) -> AppResult<SectionVersionView> {
    require(principal, Capability::SectionWrite)?;
    if let Some(cap) = input.capacity {
        validate_capacity(cap)?;
    }
    validate_optional("location", input.location.as_deref(), LOCATION_MAX)?;
    validate_optional("schedule_note", input.schedule_note.as_deref(), SCHEDULE_MAX)?;
    validate_optional("notes", input.notes.as_deref(), NOTES_MAX)?;

    ensure_section_in_scope(pool, principal, section_id).await?;

    // The parent section row's capacity must stay in sync with the latest
    // requested capacity for anti-duplicate checks (capacity lives on the
    // parent table, not the version — version carries only location/schedule/notes).
    if let Some(cap) = input.capacity {
        sqlx::query("UPDATE sections SET capacity = ? WHERE id = ?")
            .bind(cap)
            .bind(section_id.to_string())
            .execute(pool)
            .await
            .map_err(|e| AppError::Database(format!("capacity update: {}", e)))?;
    }
    if let Some(inst) = input.instructor_id {
        // Re-assignment honours the instructor-only scope rule.
        if principal.has_role(Role::Instructor)
            && !principal.is_admin()
            && !principal.has_role(Role::DepartmentHead)
            && inst != principal.user_id
        {
            return Err(AppError::Forbidden);
        }
        sqlx::query("UPDATE sections SET instructor_id = ? WHERE id = ?")
            .bind(inst.to_string())
            .bind(section_id.to_string())
            .execute(pool)
            .await
            .map_err(|e| AppError::Database(format!("instructor update: {}", e)))?;
    }

    let mut tx = pool
        .begin()
        .await
        .map_err(|e| AppError::Database(format!("draft tx: {}", e)))?;

    let max_row = sqlx::query(
        r#"SELECT COALESCE(MAX(version_number), 0) AS max_ver
             FROM section_versions
            WHERE section_id = ?
            FOR UPDATE"#,
    )
    .bind(section_id.to_string())
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| AppError::Database(format!("max ver: {}", e)))?;
    let max_ver: i32 = max_row
        .try_get("max_ver")
        .map_err(|e| AppError::Database(e.to_string()))?;
    if max_ver == 0 {
        return Err(AppError::NotFound(format!("section {}", section_id)));
    }
    let new_ver = max_ver + 1;
    let version_id = Uuid::new_v4();

    let (encrypted_notes, notes_flag): (Option<String>, i8) = match enc.encrypt_opt(input.notes.as_deref())? {
        Some(ref v) if FieldEncryption::is_encrypted(v) => (Some(v.clone()), 1),
        other => (other, 0),
    };

    sqlx::query(
        r#"INSERT INTO section_versions
           (id, section_id, version_number, location, schedule_json, notes, notes_encrypted, state, created_by)
           VALUES (?, ?, ?, ?, CAST(? AS JSON), ?, ?, 'draft', ?)"#,
    )
    .bind(version_id.to_string())
    .bind(section_id.to_string())
    .bind(new_ver)
    .bind(input.location.as_deref())
    .bind(
        input
            .schedule_note
            .as_deref()
            .map(|s| serde_json::json!({ "note": s }).to_string())
            .unwrap_or_else(|| "null".to_string()),
    )
    .bind(encrypted_notes.as_deref())
    .bind(notes_flag)
    .bind(principal.user_id.to_string())
    .execute(&mut *tx)
    .await
    .map_err(|e| AppError::Database(format!("insert draft: {}", e)))?;

    sqlx::query("UPDATE sections SET latest_version_id = ? WHERE id = ?")
        .bind(version_id.to_string())
        .bind(section_id.to_string())
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
            action: "section.draft.create",
            target_entity_type: Some("section_version"),
            target_entity_id: Some(version_id),
            change_payload: Some(serde_json::json!({
                "section_id": section_id,
                "version_number": new_ver,
            })),
            ip_address: None,
            user_agent: None,
        },
    )
    .await?;

    load_version_by_id(pool, version_id, enc).await
}

pub async fn approve_version(
    pool: &MySqlPool,
    principal: &Principal,
    section_id: Uuid,
    version_id: Uuid,
    enc: &FieldEncryption,
) -> AppResult<SectionVersionView> {
    require(principal, Capability::SectionApprove)?;
    ensure_section_in_scope(pool, principal, section_id).await?;
    let existing = load_version_by_id(pool, version_id, enc).await?;
    if existing.section_id != section_id {
        return Err(AppError::NotFound(format!("section_version {}", version_id)));
    }
    validate_transition(existing.state, VersionState::Approved)?;

    sqlx::query(
        r#"UPDATE section_versions
              SET state='approved', approved_by=?, approved_at=NOW()
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
            action: "section.approve",
            target_entity_type: Some("section_version"),
            target_entity_id: Some(version_id),
            change_payload: Some(serde_json::json!({ "section_id": section_id })),
            ip_address: None,
            user_agent: None,
        },
    )
    .await?;
    load_version_by_id(pool, version_id, enc).await
}

pub async fn publish_version(
    pool: &MySqlPool,
    principal: &Principal,
    section_id: Uuid,
    version_id: Uuid,
    enc: &FieldEncryption,
) -> AppResult<SectionView> {
    require(principal, Capability::SectionPublish)?;
    ensure_section_in_scope(pool, principal, section_id).await?;

    let mut tx = pool
        .begin()
        .await
        .map_err(|e| AppError::Database(format!("publish tx: {}", e)))?;

    let row = sqlx::query(
        r#"SELECT state, section_id FROM section_versions WHERE id=? FOR UPDATE"#,
    )
    .bind(version_id.to_string())
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| AppError::Database(format!("publish load: {}", e)))?
    .ok_or_else(|| AppError::NotFound(format!("section_version {}", version_id)))?;

    let state_s: String = row
        .try_get("state")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let sid_s: String = row
        .try_get("section_id")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let current = VersionState::from_db(&state_s).unwrap_or(VersionState::Draft);
    if sid_s != section_id.to_string() {
        return Err(AppError::NotFound(format!("section_version {}", version_id)));
    }
    validate_transition(current, VersionState::Published)?;

    sqlx::query(
        r#"UPDATE section_versions
              SET state='archived'
            WHERE section_id=? AND state='published' AND id<>?"#,
    )
    .bind(section_id.to_string())
    .bind(version_id.to_string())
    .execute(&mut *tx)
    .await
    .map_err(|e| AppError::Database(format!("archive prior: {}", e)))?;

    sqlx::query(
        r#"UPDATE section_versions
              SET state='published', published_by=?, published_at=NOW()
            WHERE id=?"#,
    )
    .bind(principal.user_id.to_string())
    .bind(version_id.to_string())
    .execute(&mut *tx)
    .await
    .map_err(|e| AppError::Database(format!("publish update: {}", e)))?;

    sqlx::query("UPDATE sections SET current_version_id=? WHERE id=?")
        .bind(version_id.to_string())
        .bind(section_id.to_string())
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::Database(format!("set baseline: {}", e)))?;

    tx.commit()
        .await
        .map_err(|e| AppError::Database(format!("publish commit: {}", e)))?;

    audit_service::record(
        pool,
        AuditEvent {
            actor_id: Some(principal.user_id),
            actor_email: Some(&principal.email),
            action: "section.publish",
            target_entity_type: Some("section_version"),
            target_entity_id: Some(version_id),
            change_payload: Some(serde_json::json!({ "section_id": section_id })),
            ip_address: None,
            user_agent: None,
        },
    )
    .await?;

    get_section_by_id(pool, principal, section_id, enc).await
}

pub async fn list_sections(
    pool: &MySqlPool,
    principal: &Principal,
    course_id: Option<Uuid>,
    department_id: Option<Uuid>,
    limit: u32,
    offset: u32,
    enc: &FieldEncryption,
) -> AppResult<Vec<SectionView>> {
    require(principal, Capability::SectionRead)?;
    let editor = principal_can(principal, Capability::SectionWrite);
    let limit = limit.clamp(1, 500) as i64;
    let offset = offset as i64;

    let scope_dept: Option<Uuid> = if principal.is_admin() || principal.has_role(Role::Librarian) {
        department_id
    } else {
        principal.department_id
    };

    let mut sql = String::from(
        r#"SELECT s.id
             FROM sections s
             JOIN courses c ON c.id = s.course_id
            WHERE 1=1"#,
    );
    if !editor {
        sql.push_str(" AND s.current_version_id IS NOT NULL");
    }
    if course_id.is_some() {
        sql.push_str(" AND s.course_id = ?");
    }
    if scope_dept.is_some() {
        sql.push_str(" AND c.department_id = ?");
    }
    sql.push_str(" ORDER BY s.year DESC, s.term ASC, s.section_code ASC LIMIT ? OFFSET ?");

    let mut q = sqlx::query(&sql);
    if let Some(cid) = course_id {
        q = q.bind(cid.to_string());
    }
    if let Some(d) = scope_dept {
        q = q.bind(d.to_string());
    }
    q = q.bind(limit).bind(offset);

    let rows = q
        .fetch_all(pool)
        .await
        .map_err(|e| AppError::Database(format!("list_sections: {}", e)))?;

    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let id: String = row
            .try_get("id")
            .map_err(|e| AppError::Database(e.to_string()))?;
        let sid = Uuid::parse_str(&id).map_err(|e| AppError::Database(e.to_string()))?;
        out.push(get_section_by_id(pool, principal, sid, enc).await?);
    }
    Ok(out)
}

pub async fn get_section_by_id(
    pool: &MySqlPool,
    principal: &Principal,
    section_id: Uuid,
    enc: &FieldEncryption,
) -> AppResult<SectionView> {
    require(principal, Capability::SectionRead)?;

    let row = sqlx::query(
        r#"SELECT s.id, s.course_id, s.section_code, s.term, s.year,
                  s.capacity, s.instructor_id, s.is_active,
                  s.current_version_id, s.latest_version_id,
                  s.created_at, s.updated_at,
                  c.code AS course_code, c.department_id
             FROM sections s
             JOIN courses c ON c.id = s.course_id
            WHERE s.id = ?"#,
    )
    .bind(section_id.to_string())
    .fetch_optional(pool)
    .await
    .map_err(|e| AppError::Database(format!("get_section: {}", e)))?
    .ok_or_else(|| AppError::NotFound(format!("section {}", section_id)))?;

    let course_id: String = row
        .try_get("course_id")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let section_code: String = row
        .try_get("section_code")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let term: String = row
        .try_get("term")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let year: i32 = row
        .try_get("year")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let capacity: Option<i32> = row
        .try_get("capacity")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let instructor_id: Option<String> = row
        .try_get("instructor_id")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let is_active: bool = row
        .try_get::<i8, _>("is_active")
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
    let course_code: String = row
        .try_get("course_code")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let department_id: Option<String> = row
        .try_get("department_id")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let course_uuid = Uuid::parse_str(&course_id)
        .map_err(|e| AppError::Database(e.to_string()))?;
    let dept = department_id.as_deref().and_then(|s| Uuid::parse_str(s).ok());

    // Department scope for reads.
    if !principal.is_admin() && !principal.has_role(Role::Librarian) {
        match (principal.department_id, dept) {
            (Some(caller), Some(section_dept)) if caller == section_dept => {}
            _ => return Err(AppError::Forbidden),
        }
    }

    let editor = principal_can(principal, Capability::SectionWrite);
    let effective_id = if editor {
        latest_version_id.clone().or_else(|| current_version_id.clone())
    } else {
        current_version_id.clone()
    };
    let effective_version = match effective_id {
        Some(ref s) => {
            let id = Uuid::parse_str(s).map_err(|e| AppError::Database(e.to_string()))?;
            Some(load_version_by_id(pool, id, enc).await?)
        }
        None => None,
    };
    if !editor && effective_version.is_none() {
        return Err(AppError::NotFound(format!("section {}", section_id)));
    }

    Ok(SectionView {
        id: section_id,
        course_id: course_uuid,
        course_code,
        department_id: dept,
        section_code,
        term,
        year,
        capacity,
        instructor_id: instructor_id
            .as_deref()
            .and_then(|s| Uuid::parse_str(s).ok()),
        is_active,
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
    section_id: Uuid,
    enc: &FieldEncryption,
) -> AppResult<Vec<SectionVersionView>> {
    require(principal, Capability::SectionWrite)?;
    ensure_section_in_scope(pool, principal, section_id).await?;
    let rows = sqlx::query(
        r#"SELECT id FROM section_versions
            WHERE section_id = ?
            ORDER BY version_number DESC"#,
    )
    .bind(section_id.to_string())
    .fetch_all(pool)
    .await
    .map_err(|e| AppError::Database(format!("list_versions: {}", e)))?;
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let id: String = row
            .try_get("id")
            .map_err(|e| AppError::Database(e.to_string()))?;
        let vid = Uuid::parse_str(&id).map_err(|e| AppError::Database(e.to_string()))?;
        out.push(load_version_by_id(pool, vid, enc).await?);
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

async fn load_course_meta(
    pool: &MySqlPool,
    course_id: Uuid,
) -> AppResult<(String, Option<Uuid>)> {
    let row = sqlx::query("SELECT code, department_id FROM courses WHERE id = ?")
        .bind(course_id.to_string())
        .fetch_optional(pool)
        .await
        .map_err(|e| AppError::Database(format!("course meta: {}", e)))?
        .ok_or_else(|| AppError::NotFound(format!("course {}", course_id)))?;
    let code: String = row
        .try_get("code")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let dept: Option<String> = row
        .try_get("department_id")
        .map_err(|e| AppError::Database(e.to_string()))?;
    Ok((
        code,
        dept.as_deref().and_then(|s| Uuid::parse_str(s).ok()),
    ))
}

fn ensure_dept_scope(
    principal: &Principal,
    course_dept: Option<Uuid>,
) -> AppResult<()> {
    if principal.is_admin() {
        return Ok(());
    }
    match (principal.department_id, course_dept) {
        (Some(caller), Some(course_dept)) if caller == course_dept => Ok(()),
        _ => Err(AppError::Forbidden),
    }
}

async fn ensure_section_in_scope(
    pool: &MySqlPool,
    principal: &Principal,
    section_id: Uuid,
) -> AppResult<()> {
    if principal.is_admin() {
        return Ok(());
    }
    let row = sqlx::query(
        r#"SELECT c.department_id, s.instructor_id
             FROM sections s
             JOIN courses c ON c.id = s.course_id
            WHERE s.id = ?"#,
    )
    .bind(section_id.to_string())
    .fetch_optional(pool)
    .await
    .map_err(|e| AppError::Database(format!("section scope: {}", e)))?
    .ok_or_else(|| AppError::NotFound(format!("section {}", section_id)))?;

    let dept: Option<String> = row
        .try_get("department_id")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let instr: Option<String> = row
        .try_get("instructor_id")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let dept_uuid = dept.as_deref().and_then(|s| Uuid::parse_str(s).ok());
    let instr_uuid = instr.as_deref().and_then(|s| Uuid::parse_str(s).ok());

    let in_dept = matches!((principal.department_id, dept_uuid),
        (Some(caller), Some(sd)) if caller == sd);

    if principal.has_role(Role::DepartmentHead) && in_dept {
        return Ok(());
    }
    if principal.has_role(Role::Instructor) {
        if instr_uuid == Some(principal.user_id) {
            return Ok(());
        }
    }
    Err(AppError::Forbidden)
}

async fn load_version_by_id(
    pool: &MySqlPool,
    version_id: Uuid,
    enc: &FieldEncryption,
) -> AppResult<SectionVersionView> {
    let row = sqlx::query(
        r#"SELECT id, section_id, version_number, location,
                  CAST(schedule_json AS CHAR) AS schedule_text,
                  notes, notes_encrypted, state, created_by, created_at,
                  approved_at, published_at
             FROM section_versions
            WHERE id = ?"#,
    )
    .bind(version_id.to_string())
    .fetch_optional(pool)
    .await
    .map_err(|e| AppError::Database(format!("load_version: {}", e)))?
    .ok_or_else(|| AppError::NotFound(format!("section_version {}", version_id)))?;

    let id: String = row.try_get("id").map_err(|e| AppError::Database(e.to_string()))?;
    let section_id: String = row
        .try_get("section_id")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let version_number: i32 = row
        .try_get("version_number")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let location: Option<String> = row
        .try_get("location")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let schedule_text: Option<String> = row
        .try_get("schedule_text")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let notes_encrypted_flag: bool = row
        .try_get::<i8, _>("notes_encrypted")
        .map(|v| v != 0)
        .map_err(|e| AppError::Database(e.to_string()))?;
    let notes_raw: Option<String> = row
        .try_get("notes")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let notes = if notes_encrypted_flag {
        enc.decrypt_opt(notes_raw.as_deref())?
    } else {
        notes_raw
    };
    let state_s: String = row
        .try_get("state")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let created_by: Option<String> = row
        .try_get("created_by")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let created_at: NaiveDateTime = row
        .try_get("created_at")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let approved_at: Option<NaiveDateTime> = row
        .try_get("approved_at")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let published_at: Option<NaiveDateTime> = row
        .try_get("published_at")
        .map_err(|e| AppError::Database(e.to_string()))?;

    let schedule_note = schedule_text
        .as_deref()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok())
        .and_then(|v| v.get("note").and_then(|n| n.as_str().map(String::from)));

    Ok(SectionVersionView {
        id: Uuid::parse_str(&id).map_err(|e| AppError::Database(e.to_string()))?,
        section_id: Uuid::parse_str(&section_id)
            .map_err(|e| AppError::Database(e.to_string()))?,
        version_number,
        location,
        schedule_note,
        notes,
        state: VersionState::from_db(&state_s).unwrap_or(VersionState::Draft),
        created_by: created_by.as_deref().and_then(|s| Uuid::parse_str(s).ok()),
        created_at,
        approved_at,
        published_at,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn term_validation() {
        for t in VALID_TERMS {
            assert!(is_valid_term(t));
        }
        assert!(is_valid_term("Fall"));
        assert!(is_valid_term("FALL"));
        assert!(!is_valid_term("autumn"));
        assert!(!is_valid_term("q1"));
        assert_eq!(normalize_term(" Fall "), "fall");
    }

    #[test]
    fn section_code_validation() {
        assert!(is_valid_section_code("A1"));
        assert!(is_valid_section_code("SEC-01"));
        assert!(is_valid_section_code("S_001"));
        assert!(!is_valid_section_code(""));
        assert!(!is_valid_section_code("has space"));
        assert!(!is_valid_section_code("toolongsectioncodeoverlimit12345"));
    }

    #[test]
    fn capacity_bounds() {
        assert!(validate_capacity(0).is_err());
        assert!(validate_capacity(1).is_ok());
        assert!(validate_capacity(500).is_ok());
        assert!(validate_capacity(1000).is_ok());
        assert!(validate_capacity(1001).is_err());
    }

    #[test]
    fn year_bounds() {
        assert!(validate_year(1999).is_err());
        assert!(validate_year(2000).is_ok());
        assert!(validate_year(2026).is_ok());
        assert!(validate_year(2100).is_ok());
        assert!(validate_year(2101).is_err());
    }

    #[test]
    fn term_normalization_round_trip() {
        // Each canonical lowercase variant, upper-cased, must normalize
        // back to the exact canonical value.
        for t in VALID_TERMS {
            let upper = t.to_uppercase();
            assert_eq!(normalize_term(&upper), *t);
            assert!(is_valid_term(&upper));
        }
    }

    #[test]
    fn section_code_accepts_common_patterns() {
        assert!(is_valid_section_code("001"));
        assert!(is_valid_section_code("SEC-01"));
        assert!(is_valid_section_code("A1"));
        assert!(is_valid_section_code("EVE-LAB"));
    }

    #[test]
    fn capacity_edge_cases() {
        assert!(validate_capacity(0).is_err());
        assert!(validate_capacity(1).is_ok());
        assert!(validate_capacity(1000).is_ok());
        assert!(validate_capacity(1001).is_err());
    }

    #[test]
    fn year_edge_cases() {
        assert!(validate_year(2000).is_ok());
        assert!(validate_year(2100).is_ok());
        assert!(validate_year(1999).is_err());
        assert!(validate_year(2101).is_err());
    }
}
