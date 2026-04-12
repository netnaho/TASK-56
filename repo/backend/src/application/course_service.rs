//! Course catalog CRUD, versioning, and prerequisites.
//!
//! Follows the same two-pointer + state-machine design as
//! [`super::journal_service`] and [`super::resource_service`]:
//!
//! * `courses.current_version_id`  — published baseline
//! * `courses.latest_version_id`   — head of the edit chain
//! * `course_versions.state`       — draft → approved → published → archived
//!
//! Prerequisites are an AND-list: to satisfy a course, the learner must
//! have completed every row in `course_prerequisites` for that course.
//! Self-references and direct duplicates are rejected; indirect cycles
//! are rejected via a DFS reachability check in [`ensure_no_cycle`].

use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use sqlx::{MySqlPool, Row};
use std::collections::HashSet;
use uuid::Uuid;

use super::audit_service::{self, AuditEvent};
use super::authorization::{principal_can, require, Capability};
use super::principal::{Principal, Role};
use crate::domain::versioning::{validate_transition, VersionState};
use crate::errors::{AppError, AppResult};

// ---------------------------------------------------------------------------
// Inputs and view models
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct CourseCreateInput {
    pub code: String,
    pub title: String,
    pub department_id: Option<Uuid>,
    pub description: Option<String>,
    pub syllabus: Option<String>,
    pub credit_hours: f32,
    pub contact_hours: f32,
    pub change_summary: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CourseEditInput {
    pub description: Option<String>,
    pub syllabus: Option<String>,
    pub credit_hours: f32,
    pub contact_hours: f32,
    pub change_summary: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CourseView {
    pub id: Uuid,
    pub code: String,
    pub title: String,
    pub department_id: Option<Uuid>,
    pub owner_id: Option<Uuid>,
    pub is_active: bool,
    pub current_version_id: Option<Uuid>,
    pub latest_version_id: Option<Uuid>,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
    pub effective_version: Option<CourseVersionView>,
    pub prerequisites: Vec<PrerequisiteRef>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CourseVersionView {
    pub id: Uuid,
    pub course_id: Uuid,
    pub version_number: i32,
    pub description: Option<String>,
    pub syllabus: Option<String>,
    pub credit_hours: Option<f32>,
    pub contact_hours: Option<f32>,
    pub change_summary: Option<String>,
    pub state: VersionState,
    pub created_by: Option<Uuid>,
    pub created_at: NaiveDateTime,
    pub approved_at: Option<NaiveDateTime>,
    pub published_at: Option<NaiveDateTime>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PrerequisiteRef {
    pub prerequisite_course_id: Uuid,
    pub prerequisite_code: String,
    pub min_grade: Option<String>,
}

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

const TITLE_MIN: usize = 3;
const TITLE_MAX: usize = 500;
const CODE_MIN: usize = 3;
const CODE_MAX: usize = 50;
const DESCRIPTION_MAX: usize = 4_000;
const SYLLABUS_MAX: usize = 500_000;
const CHANGE_SUMMARY_MAX: usize = 2_000;
const CREDIT_HOURS_MIN: f32 = 0.5;
const CREDIT_HOURS_MAX: f32 = 20.0;
const CONTACT_HOURS_MIN: f32 = 0.5;
const CONTACT_HOURS_MAX: f32 = 30.0;

/// Course code: uppercase letters followed by digits, optionally with an
/// intermediate dash/underscore and an optional trailing uppercase letter.
/// Matches `CS101`, `CS-101`, `MATH-210A`, `BIOL3001`.
pub fn is_valid_course_code(code: &str) -> bool {
    let trimmed = code.trim();
    if trimmed.len() < CODE_MIN || trimmed.len() > CODE_MAX {
        return false;
    }
    let bytes = trimmed.as_bytes();
    let mut i = 0;
    let mut letters = 0;
    while i < bytes.len() && (bytes[i] as char).is_ascii_uppercase() {
        letters += 1;
        i += 1;
    }
    if !(2..=5).contains(&letters) {
        return false;
    }
    if i < bytes.len() && (bytes[i] == b'-' || bytes[i] == b'_') {
        i += 1;
    }
    let mut digits = 0;
    while i < bytes.len() && (bytes[i] as char).is_ascii_digit() {
        digits += 1;
        i += 1;
    }
    if !(3..=4).contains(&digits) {
        return false;
    }
    if i < bytes.len() && (bytes[i] as char).is_ascii_uppercase() {
        i += 1;
    }
    i == bytes.len()
}

fn validate_title(title: &str) -> AppResult<()> {
    let len = title.trim().chars().count();
    if len < TITLE_MIN || len > TITLE_MAX {
        return Err(AppError::Validation(format!(
            "title must be {}-{} characters",
            TITLE_MIN, TITLE_MAX
        )));
    }
    Ok(())
}

fn validate_code(code: &str) -> AppResult<()> {
    if !is_valid_course_code(code) {
        return Err(AppError::Validation(format!(
            "course code '{}' is not in the expected format (e.g. CS101 or MATH-210A)",
            code
        )));
    }
    Ok(())
}

pub fn validate_credit_hours(v: f32) -> AppResult<()> {
    if !v.is_finite() || v < CREDIT_HOURS_MIN || v > CREDIT_HOURS_MAX {
        return Err(AppError::Validation(format!(
            "credit_hours must be between {} and {}",
            CREDIT_HOURS_MIN, CREDIT_HOURS_MAX
        )));
    }
    Ok(())
}

pub fn validate_contact_hours(v: f32) -> AppResult<()> {
    if !v.is_finite() || v < CONTACT_HOURS_MIN || v > CONTACT_HOURS_MAX {
        return Err(AppError::Validation(format!(
            "contact_hours must be between {} and {}",
            CONTACT_HOURS_MIN, CONTACT_HOURS_MAX
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

pub async fn create_course(
    pool: &MySqlPool,
    principal: &Principal,
    input: CourseCreateInput,
) -> AppResult<CourseView> {
    require(principal, Capability::CourseWrite)?;
    validate_code(&input.code)?;
    validate_title(&input.title)?;
    validate_optional("description", input.description.as_deref(), DESCRIPTION_MAX)?;
    validate_optional("syllabus", input.syllabus.as_deref(), SYLLABUS_MAX)?;
    validate_optional(
        "change_summary",
        input.change_summary.as_deref(),
        CHANGE_SUMMARY_MAX,
    )?;
    validate_credit_hours(input.credit_hours)?;
    validate_contact_hours(input.contact_hours)?;

    // Department scope: non-admin callers can only create courses in
    // their own department.
    if !principal.is_admin() {
        match (principal.department_id, input.department_id) {
            (Some(caller_dept), Some(wanted_dept)) if caller_dept == wanted_dept => {}
            (Some(_), None) => {} // default to caller's dept below
            _ => return Err(AppError::Forbidden),
        }
    }
    let effective_dept = input.department_id.or(principal.department_id);

    // Friendly 409 before hitting the unique-key constraint.
    let existing = sqlx::query("SELECT 1 FROM courses WHERE code = ? LIMIT 1")
        .bind(input.code.trim())
        .fetch_optional(pool)
        .await
        .map_err(|e| AppError::Database(format!("course code check: {}", e)))?;
    if existing.is_some() {
        return Err(AppError::Conflict(format!(
            "course code '{}' already exists",
            input.code.trim()
        )));
    }

    let mut tx = pool
        .begin()
        .await
        .map_err(|e| AppError::Database(format!("create_course tx: {}", e)))?;

    let course_id = Uuid::new_v4();
    let version_id = Uuid::new_v4();

    sqlx::query(
        r#"INSERT INTO courses (id, code, title, department_id, owner_id, is_active)
           VALUES (?, ?, ?, ?, ?, TRUE)"#,
    )
    .bind(course_id.to_string())
    .bind(input.code.trim())
    .bind(input.title.trim())
    .bind(effective_dept.map(|u| u.to_string()))
    .bind(principal.user_id.to_string())
    .execute(&mut *tx)
    .await
    .map_err(|e| AppError::Database(format!("create course insert: {}", e)))?;

    sqlx::query(
        r#"INSERT INTO course_versions
           (id, course_id, version_number, description, syllabus,
            credit_hours, contact_hours, change_summary, state, created_by)
           VALUES (?, ?, 1, ?, ?, ?, ?, ?, 'draft', ?)"#,
    )
    .bind(version_id.to_string())
    .bind(course_id.to_string())
    .bind(input.description.as_deref())
    .bind(input.syllabus.as_deref())
    .bind(input.credit_hours)
    .bind(input.contact_hours)
    .bind(input.change_summary.as_deref())
    .bind(principal.user_id.to_string())
    .execute(&mut *tx)
    .await
    .map_err(|e| AppError::Database(format!("create course_version insert: {}", e)))?;

    sqlx::query("UPDATE courses SET latest_version_id = ? WHERE id = ?")
        .bind(version_id.to_string())
        .bind(course_id.to_string())
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::Database(format!("update latest: {}", e)))?;

    tx.commit()
        .await
        .map_err(|e| AppError::Database(format!("create_course commit: {}", e)))?;

    audit_service::record(
        pool,
        AuditEvent {
            actor_id: Some(principal.user_id),
            actor_email: Some(&principal.email),
            action: "course.create",
            target_entity_type: Some("course"),
            target_entity_id: Some(course_id),
            change_payload: Some(serde_json::json!({
                "code": input.code.trim(),
                "title": input.title.trim(),
                "credit_hours": input.credit_hours,
                "contact_hours": input.contact_hours,
            })),
            ip_address: None,
            user_agent: None,
        },
    )
    .await?;

    get_course_by_id(pool, principal, course_id).await
}

pub async fn create_draft_version(
    pool: &MySqlPool,
    principal: &Principal,
    course_id: Uuid,
    input: CourseEditInput,
) -> AppResult<CourseVersionView> {
    require(principal, Capability::CourseWrite)?;
    validate_optional("description", input.description.as_deref(), DESCRIPTION_MAX)?;
    validate_optional("syllabus", input.syllabus.as_deref(), SYLLABUS_MAX)?;
    validate_optional(
        "change_summary",
        input.change_summary.as_deref(),
        CHANGE_SUMMARY_MAX,
    )?;
    validate_credit_hours(input.credit_hours)?;
    validate_contact_hours(input.contact_hours)?;

    ensure_course_in_scope(pool, principal, course_id).await?;

    let mut tx = pool
        .begin()
        .await
        .map_err(|e| AppError::Database(format!("draft tx: {}", e)))?;

    let max_row = sqlx::query(
        r#"SELECT COALESCE(MAX(version_number), 0) AS max_ver
             FROM course_versions
            WHERE course_id = ?
            FOR UPDATE"#,
    )
    .bind(course_id.to_string())
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| AppError::Database(format!("max ver: {}", e)))?;
    let max_ver: i32 = max_row
        .try_get("max_ver")
        .map_err(|e| AppError::Database(e.to_string()))?;
    if max_ver == 0 {
        return Err(AppError::NotFound(format!("course {}", course_id)));
    }
    let new_ver = max_ver + 1;
    let version_id = Uuid::new_v4();

    sqlx::query(
        r#"INSERT INTO course_versions
           (id, course_id, version_number, description, syllabus,
            credit_hours, contact_hours, change_summary, state, created_by)
           VALUES (?, ?, ?, ?, ?, ?, ?, ?, 'draft', ?)"#,
    )
    .bind(version_id.to_string())
    .bind(course_id.to_string())
    .bind(new_ver)
    .bind(input.description.as_deref())
    .bind(input.syllabus.as_deref())
    .bind(input.credit_hours)
    .bind(input.contact_hours)
    .bind(input.change_summary.as_deref())
    .bind(principal.user_id.to_string())
    .execute(&mut *tx)
    .await
    .map_err(|e| AppError::Database(format!("insert draft: {}", e)))?;

    sqlx::query("UPDATE courses SET latest_version_id = ? WHERE id = ?")
        .bind(version_id.to_string())
        .bind(course_id.to_string())
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
            action: "course.draft.create",
            target_entity_type: Some("course_version"),
            target_entity_id: Some(version_id),
            change_payload: Some(serde_json::json!({
                "course_id": course_id,
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
    course_id: Uuid,
    version_id: Uuid,
) -> AppResult<CourseVersionView> {
    require(principal, Capability::CourseApprove)?;
    ensure_course_in_scope(pool, principal, course_id).await?;
    let existing = load_version_by_id(pool, version_id).await?;
    if existing.course_id != course_id {
        return Err(AppError::NotFound(format!("course_version {}", version_id)));
    }
    validate_transition(existing.state, VersionState::Approved)?;

    sqlx::query(
        r#"UPDATE course_versions
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
            action: "course.approve",
            target_entity_type: Some("course_version"),
            target_entity_id: Some(version_id),
            change_payload: Some(serde_json::json!({ "course_id": course_id })),
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
    course_id: Uuid,
    version_id: Uuid,
) -> AppResult<CourseView> {
    require(principal, Capability::CoursePublish)?;
    ensure_course_in_scope(pool, principal, course_id).await?;

    let mut tx = pool
        .begin()
        .await
        .map_err(|e| AppError::Database(format!("publish tx: {}", e)))?;

    let row = sqlx::query(
        r#"SELECT state, course_id FROM course_versions WHERE id=? FOR UPDATE"#,
    )
    .bind(version_id.to_string())
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| AppError::Database(format!("publish load: {}", e)))?
    .ok_or_else(|| AppError::NotFound(format!("course_version {}", version_id)))?;

    let state_s: String = row
        .try_get("state")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let cid_s: String = row
        .try_get("course_id")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let current = VersionState::from_db(&state_s).unwrap_or(VersionState::Draft);
    if cid_s != course_id.to_string() {
        return Err(AppError::NotFound(format!("course_version {}", version_id)));
    }
    validate_transition(current, VersionState::Published)?;

    sqlx::query(
        r#"UPDATE course_versions
              SET state='archived'
            WHERE course_id=? AND state='published' AND id<>?"#,
    )
    .bind(course_id.to_string())
    .bind(version_id.to_string())
    .execute(&mut *tx)
    .await
    .map_err(|e| AppError::Database(format!("archive prior: {}", e)))?;

    sqlx::query(
        r#"UPDATE course_versions
              SET state='published', published_by=?, published_at=NOW()
            WHERE id=?"#,
    )
    .bind(principal.user_id.to_string())
    .bind(version_id.to_string())
    .execute(&mut *tx)
    .await
    .map_err(|e| AppError::Database(format!("publish update: {}", e)))?;

    sqlx::query("UPDATE courses SET current_version_id=? WHERE id=?")
        .bind(version_id.to_string())
        .bind(course_id.to_string())
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
            action: "course.publish",
            target_entity_type: Some("course_version"),
            target_entity_id: Some(version_id),
            change_payload: Some(serde_json::json!({ "course_id": course_id })),
            ip_address: None,
            user_agent: None,
        },
    )
    .await?;

    get_course_by_id(pool, principal, course_id).await
}

pub async fn list_courses(
    pool: &MySqlPool,
    principal: &Principal,
    department_id: Option<Uuid>,
    limit: u32,
    offset: u32,
) -> AppResult<Vec<CourseView>> {
    require(principal, Capability::CourseRead)?;
    let editor = principal_can(principal, Capability::CourseWrite);
    let limit = limit.clamp(1, 500) as i64;
    let offset = offset as i64;

    // Department scope:
    //   Admin / Librarian see everything unless a department filter is
    //   explicitly supplied. Everyone else sees only their department.
    let scope_dept: Option<Uuid> = if principal.is_admin() || principal.has_role(Role::Librarian) {
        department_id
    } else {
        principal.department_id
    };

    let mut sql = String::from("SELECT id FROM courses WHERE 1=1");
    if !editor {
        sql.push_str(" AND current_version_id IS NOT NULL");
    }
    if scope_dept.is_some() {
        sql.push_str(" AND department_id = ?");
    }
    sql.push_str(" ORDER BY code ASC LIMIT ? OFFSET ?");

    let mut q = sqlx::query(&sql);
    if let Some(d) = scope_dept {
        q = q.bind(d.to_string());
    }
    q = q.bind(limit).bind(offset);

    let rows = q
        .fetch_all(pool)
        .await
        .map_err(|e| AppError::Database(format!("list_courses: {}", e)))?;

    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let id: String = row
            .try_get("id")
            .map_err(|e| AppError::Database(e.to_string()))?;
        let uid = Uuid::parse_str(&id).map_err(|e| AppError::Database(e.to_string()))?;
        out.push(get_course_by_id(pool, principal, uid).await?);
    }
    Ok(out)
}

pub async fn get_course_by_id(
    pool: &MySqlPool,
    principal: &Principal,
    course_id: Uuid,
) -> AppResult<CourseView> {
    require(principal, Capability::CourseRead)?;
    let row = sqlx::query(
        r#"SELECT id, code, title, department_id, owner_id, is_active,
                  current_version_id, latest_version_id, created_at, updated_at
             FROM courses WHERE id = ?"#,
    )
    .bind(course_id.to_string())
    .fetch_optional(pool)
    .await
    .map_err(|e| AppError::Database(format!("get_course: {}", e)))?
    .ok_or_else(|| AppError::NotFound(format!("course {}", course_id)))?;

    let code: String = row
        .try_get("code")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let title: String = row
        .try_get("title")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let department_id: Option<String> = row
        .try_get("department_id")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let owner_id: Option<String> = row
        .try_get("owner_id")
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

    let dept = department_id.as_deref().and_then(|s| Uuid::parse_str(s).ok());

    // Department scope: non-admin/non-librarian callers see only their own
    // department's courses.
    if !principal.is_admin() && !principal.has_role(Role::Librarian) {
        match (principal.department_id, dept) {
            (Some(caller), Some(course_dept)) if caller == course_dept => {}
            _ => return Err(AppError::Forbidden),
        }
    }

    let editor = principal_can(principal, Capability::CourseWrite);
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
        return Err(AppError::NotFound(format!("course {}", course_id)));
    }

    let prerequisites = list_prerequisites_internal(pool, course_id).await?;

    Ok(CourseView {
        id: course_id,
        code,
        title,
        department_id: dept,
        owner_id: owner_id.as_deref().and_then(|s| Uuid::parse_str(s).ok()),
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
        prerequisites,
    })
}

/// Looks up a course by its code. Returns `(course_id, department_id)`
/// if found; used by the import pipeline for prerequisite resolution.
pub async fn find_by_code(
    pool: &MySqlPool,
    code: &str,
) -> AppResult<Option<(Uuid, Option<Uuid>)>> {
    let row = sqlx::query("SELECT id, department_id FROM courses WHERE code = ? LIMIT 1")
        .bind(code.trim())
        .fetch_optional(pool)
        .await
        .map_err(|e| AppError::Database(format!("find_by_code: {}", e)))?;
    let Some(row) = row else { return Ok(None) };
    let id_s: String = row.try_get("id").map_err(|e| AppError::Database(e.to_string()))?;
    let dept: Option<String> = row
        .try_get("department_id")
        .map_err(|e| AppError::Database(e.to_string()))?;
    Ok(Some((
        Uuid::parse_str(&id_s).map_err(|e| AppError::Database(e.to_string()))?,
        dept.as_deref().and_then(|s| Uuid::parse_str(s).ok()),
    )))
}

pub async fn list_versions(
    pool: &MySqlPool,
    principal: &Principal,
    course_id: Uuid,
) -> AppResult<Vec<CourseVersionView>> {
    require(principal, Capability::CourseWrite)?;
    ensure_course_in_scope(pool, principal, course_id).await?;
    let rows = sqlx::query(
        r#"SELECT id FROM course_versions
            WHERE course_id = ?
            ORDER BY version_number DESC"#,
    )
    .bind(course_id.to_string())
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

// ---------------------------------------------------------------------------
// Prerequisites
// ---------------------------------------------------------------------------

pub async fn add_prerequisite(
    pool: &MySqlPool,
    principal: &Principal,
    course_id: Uuid,
    prereq_id: Uuid,
    min_grade: Option<String>,
) -> AppResult<()> {
    require(principal, Capability::CourseWrite)?;
    ensure_course_in_scope(pool, principal, course_id).await?;
    if course_id == prereq_id {
        return Err(AppError::Validation(
            "a course cannot be its own prerequisite".into(),
        ));
    }

    // Both courses must exist.
    let count_row = sqlx::query(
        "SELECT COUNT(*) AS n FROM courses WHERE id IN (?, ?)",
    )
    .bind(course_id.to_string())
    .bind(prereq_id.to_string())
    .fetch_one(pool)
    .await
    .map_err(|e| AppError::Database(format!("prereq existence: {}", e)))?;
    let n: i64 = count_row
        .try_get("n")
        .map_err(|e| AppError::Database(e.to_string()))?;
    if n < 2 {
        return Err(AppError::NotFound(
            "one of the courses in the prerequisite link does not exist".into(),
        ));
    }

    // Cycle prevention.
    ensure_no_cycle(pool, course_id, prereq_id).await?;

    if let Some(ref g) = min_grade {
        if g.chars().count() > 4 {
            return Err(AppError::Validation(
                "min_grade must be at most 4 characters".into(),
            ));
        }
    }

    sqlx::query(
        r#"INSERT INTO course_prerequisites
           (course_id, prerequisite_course_id, min_grade, created_by)
           VALUES (?, ?, ?, ?)"#,
    )
    .bind(course_id.to_string())
    .bind(prereq_id.to_string())
    .bind(min_grade.as_deref())
    .bind(principal.user_id.to_string())
    .execute(pool)
    .await
    .map_err(|e| {
        if e.to_string().contains("Duplicate entry") {
            AppError::Conflict("prerequisite relationship already exists".into())
        } else {
            AppError::Database(format!("add_prerequisite: {}", e))
        }
    })?;

    audit_service::record(
        pool,
        AuditEvent {
            actor_id: Some(principal.user_id),
            actor_email: Some(&principal.email),
            action: "course.prerequisite.add",
            target_entity_type: Some("course"),
            target_entity_id: Some(course_id),
            change_payload: Some(serde_json::json!({
                "prerequisite_course_id": prereq_id,
                "min_grade": min_grade,
            })),
            ip_address: None,
            user_agent: None,
        },
    )
    .await?;
    Ok(())
}

pub async fn remove_prerequisite(
    pool: &MySqlPool,
    principal: &Principal,
    course_id: Uuid,
    prereq_id: Uuid,
) -> AppResult<()> {
    require(principal, Capability::CourseWrite)?;
    ensure_course_in_scope(pool, principal, course_id).await?;

    let result = sqlx::query(
        "DELETE FROM course_prerequisites WHERE course_id = ? AND prerequisite_course_id = ?",
    )
    .bind(course_id.to_string())
    .bind(prereq_id.to_string())
    .execute(pool)
    .await
    .map_err(|e| AppError::Database(format!("remove_prerequisite: {}", e)))?;
    if result.rows_affected() == 0 {
        return Err(AppError::NotFound("prerequisite link".into()));
    }

    audit_service::record(
        pool,
        AuditEvent {
            actor_id: Some(principal.user_id),
            actor_email: Some(&principal.email),
            action: "course.prerequisite.remove",
            target_entity_type: Some("course"),
            target_entity_id: Some(course_id),
            change_payload: Some(serde_json::json!({
                "prerequisite_course_id": prereq_id,
            })),
            ip_address: None,
            user_agent: None,
        },
    )
    .await?;
    Ok(())
}

async fn list_prerequisites_internal(
    pool: &MySqlPool,
    course_id: Uuid,
) -> AppResult<Vec<PrerequisiteRef>> {
    let rows = sqlx::query(
        r#"SELECT p.prerequisite_course_id, p.min_grade, c.code AS prereq_code
             FROM course_prerequisites p
             JOIN courses c ON c.id = p.prerequisite_course_id
            WHERE p.course_id = ?
            ORDER BY c.code ASC"#,
    )
    .bind(course_id.to_string())
    .fetch_all(pool)
    .await
    .map_err(|e| AppError::Database(format!("list_prerequisites: {}", e)))?;

    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let pid: String = row
            .try_get("prerequisite_course_id")
            .map_err(|e| AppError::Database(e.to_string()))?;
        let min_grade: Option<String> = row
            .try_get("min_grade")
            .map_err(|e| AppError::Database(e.to_string()))?;
        let prereq_code: String = row
            .try_get("prereq_code")
            .map_err(|e| AppError::Database(e.to_string()))?;
        out.push(PrerequisiteRef {
            prerequisite_course_id: Uuid::parse_str(&pid)
                .map_err(|e| AppError::Database(e.to_string()))?,
            prerequisite_code: prereq_code,
            min_grade,
        });
    }
    Ok(out)
}

/// Walk the prerequisite graph from `start_id` and error if we can reach
/// `target_id`. Used before inserting the edge `target_id -> start_id` so
/// we never create a cycle.
async fn ensure_no_cycle(
    pool: &MySqlPool,
    target_id: Uuid,
    start_id: Uuid,
) -> AppResult<()> {
    let mut stack = vec![start_id];
    let mut visited = HashSet::new();
    visited.insert(start_id);
    while let Some(current) = stack.pop() {
        if current == target_id {
            return Err(AppError::Conflict(
                "adding this prerequisite would create a cycle".into(),
            ));
        }
        let rows = sqlx::query(
            "SELECT prerequisite_course_id FROM course_prerequisites WHERE course_id = ?",
        )
        .bind(current.to_string())
        .fetch_all(pool)
        .await
        .map_err(|e| AppError::Database(format!("cycle check: {}", e)))?;
        for row in rows {
            let pid: String = row
                .try_get("prerequisite_course_id")
                .map_err(|e| AppError::Database(e.to_string()))?;
            let pid = Uuid::parse_str(&pid).map_err(|e| AppError::Database(e.to_string()))?;
            if visited.insert(pid) {
                stack.push(pid);
            }
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

async fn ensure_course_in_scope(
    pool: &MySqlPool,
    principal: &Principal,
    course_id: Uuid,
) -> AppResult<()> {
    if principal.is_admin() {
        return Ok(());
    }
    let row = sqlx::query("SELECT department_id FROM courses WHERE id = ?")
        .bind(course_id.to_string())
        .fetch_optional(pool)
        .await
        .map_err(|e| AppError::Database(format!("scope lookup: {}", e)))?
        .ok_or_else(|| AppError::NotFound(format!("course {}", course_id)))?;
    let dept: Option<String> = row
        .try_get("department_id")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let dept_uuid = dept.as_deref().and_then(|s| Uuid::parse_str(s).ok());
    match (principal.department_id, dept_uuid) {
        (Some(caller), Some(course_dept)) if caller == course_dept => Ok(()),
        _ => Err(AppError::Forbidden),
    }
}

async fn load_version_by_id(
    pool: &MySqlPool,
    version_id: Uuid,
) -> AppResult<CourseVersionView> {
    let row = sqlx::query(
        r#"SELECT id, course_id, version_number, description, syllabus,
                  credit_hours, contact_hours, change_summary, state,
                  created_by, created_at, approved_at, published_at
             FROM course_versions
            WHERE id = ?"#,
    )
    .bind(version_id.to_string())
    .fetch_optional(pool)
    .await
    .map_err(|e| AppError::Database(format!("load_version: {}", e)))?
    .ok_or_else(|| AppError::NotFound(format!("course_version {}", version_id)))?;

    let id: String = row.try_get("id").map_err(|e| AppError::Database(e.to_string()))?;
    let course_id: String = row
        .try_get("course_id")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let version_number: i32 = row
        .try_get("version_number")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let description: Option<String> = row
        .try_get("description")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let syllabus: Option<String> = row
        .try_get("syllabus")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let credit_hours: Option<f32> = row
        .try_get("credit_hours")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let contact_hours: Option<f32> = row
        .try_get("contact_hours")
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
    let approved_at: Option<NaiveDateTime> = row
        .try_get("approved_at")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let published_at: Option<NaiveDateTime> = row
        .try_get("published_at")
        .map_err(|e| AppError::Database(e.to_string()))?;

    Ok(CourseVersionView {
        id: Uuid::parse_str(&id).map_err(|e| AppError::Database(e.to_string()))?,
        course_id: Uuid::parse_str(&course_id)
            .map_err(|e| AppError::Database(e.to_string()))?,
        version_number,
        description,
        syllabus,
        credit_hours,
        contact_hours,
        change_summary,
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
    fn course_code_format_accepted_cases() {
        assert!(is_valid_course_code("CS101"));
        assert!(is_valid_course_code("CS-101"));
        assert!(is_valid_course_code("MATH210A"));
        assert!(is_valid_course_code("MATH-210A"));
        assert!(is_valid_course_code("BIOL3001"));
    }

    #[test]
    fn course_code_format_rejected_cases() {
        assert!(!is_valid_course_code(""));
        assert!(!is_valid_course_code("cs101")); // lowercase
        assert!(!is_valid_course_code("C101")); // 1 letter
        assert!(!is_valid_course_code("ABCDEF101")); // 6 letters
        assert!(!is_valid_course_code("CS1")); // 1 digit
        assert!(!is_valid_course_code("CS12345")); // 5 digits
        assert!(!is_valid_course_code("CS 101")); // space
        assert!(!is_valid_course_code("CS101AB")); // two trailing letters
    }

    #[test]
    fn credit_hours_bounds() {
        assert!(validate_credit_hours(0.4).is_err());
        assert!(validate_credit_hours(0.5).is_ok());
        assert!(validate_credit_hours(3.0).is_ok());
        assert!(validate_credit_hours(20.0).is_ok());
        assert!(validate_credit_hours(20.1).is_err());
        assert!(validate_credit_hours(f32::NAN).is_err());
        assert!(validate_credit_hours(f32::INFINITY).is_err());
    }

    #[test]
    fn contact_hours_bounds() {
        assert!(validate_contact_hours(0.4).is_err());
        assert!(validate_contact_hours(0.5).is_ok());
        assert!(validate_contact_hours(30.0).is_ok());
        assert!(validate_contact_hours(30.1).is_err());
    }

    #[test]
    fn course_code_parses_longest_valid_form() {
        // 5 letters, 4 digits, trailing uppercase letter.
        assert!(is_valid_course_code("BIOL3001A"));
        // Dash-separated form.
        assert!(is_valid_course_code("CS-101"));
    }

    #[test]
    fn course_code_rejects_mixed_case_and_spaces() {
        assert!(!is_valid_course_code("Cs101"));
        assert!(!is_valid_course_code("CS 101"));
        assert!(!is_valid_course_code("CS101 "));
    }

    #[test]
    fn credit_hours_and_contact_hours_bounds_edge() {
        // Upper bound of credit hours is inclusive at 20.0.
        assert!(validate_credit_hours(20.0).is_ok());
        // Upper bound of contact hours is inclusive at 30.0.
        assert!(validate_contact_hours(30.0).is_ok());
        // Just beyond the credit-hours ceiling.
        assert!(validate_credit_hours(20.000001).is_err());
    }
}
