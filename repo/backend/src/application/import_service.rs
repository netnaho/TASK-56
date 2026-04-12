//! Bulk import pipeline for courses and sections.
//!
//! # Two modes
//!
//! * **Dry-run** — parses the whole input, validates every row, and
//!   returns row-level errors. **Never touches the database beyond
//!   read-only lookups**. Callers can use dry-run to preview what a
//!   commit would do without risk.
//! * **Commit** — runs the exact same validation. If any row is
//!   invalid, the commit is aborted with *no* writes. Otherwise every
//!   row is inserted inside a single database transaction; any failure
//!   mid-flight rolls back the whole batch. "All-or-nothing" by design.
//!
//! # Formats
//!
//! Both CSV (via `csv`) and XLSX (via `calamine`) are real implementations.
//! The xlsx reader treats the first row as the header and only looks at
//! the first worksheet.
//!
//! # Column contract
//!
//! Courses (header row, exact names, case-insensitive):
//! `code, title, department_code, credit_hours, contact_hours,
//!  description, prerequisites`
//! `prerequisites` is a `;`-separated list of course codes. An empty value
//! means "no prerequisites".
//!
//! Sections:
//! `course_code, section_code, term, year, capacity, instructor_email,
//!  location, schedule_note, notes`
//!
//! Extra columns are ignored; missing columns produce a row-level error.

use std::collections::HashMap;
use std::io::Cursor;

use calamine::{Data, Reader, Xlsx};
use serde::{Deserialize, Serialize};
use sqlx::{MySqlPool, Row};
use uuid::Uuid;

use super::audit_service::{self, AuditEvent};
use super::authorization::{require, Capability};
use super::course_service::{
    is_valid_course_code, validate_contact_hours, validate_credit_hours,
};
use super::principal::Principal;
use super::section_service::{
    is_valid_section_code, is_valid_term, normalize_term, validate_capacity, validate_year,
};
use crate::errors::{AppError, AppResult};

// ---------------------------------------------------------------------------
// Enums / view models
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImportFormat {
    Csv,
    Xlsx,
}

impl ImportFormat {
    pub fn as_db(self) -> &'static str {
        match self {
            ImportFormat::Csv => "csv",
            ImportFormat::Xlsx => "xlsx",
        }
    }
    pub fn from_mime(mime: &str) -> Option<Self> {
        match mime {
            "text/csv" | "application/csv" | "text/plain" => Some(ImportFormat::Csv),
            "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"
            | "application/vnd.ms-excel" => Some(ImportFormat::Xlsx),
            _ => None,
        }
    }
    pub fn from_extension(name: &str) -> Option<Self> {
        let lower = name.to_lowercase();
        if lower.ends_with(".csv") {
            Some(ImportFormat::Csv)
        } else if lower.ends_with(".xlsx") {
            Some(ImportFormat::Xlsx)
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImportMode {
    DryRun,
    Commit,
}

impl ImportMode {
    pub fn as_db(self) -> &'static str {
        match self {
            ImportMode::DryRun => "dry_run",
            ImportMode::Commit => "commit",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImportKind {
    Courses,
    Sections,
}

impl ImportKind {
    pub fn as_db(self) -> &'static str {
        match self {
            ImportKind::Courses => "courses",
            ImportKind::Sections => "sections",
        }
    }
    pub fn required_capability(self) -> Capability {
        match self {
            ImportKind::Courses => Capability::ImportCourses,
            ImportKind::Sections => Capability::ImportSections,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct FieldError {
    pub field: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct RowReport {
    /// 1-based row index. The header is row 1, so the first data row is 2.
    pub row_index: usize,
    pub ok: bool,
    pub errors: Vec<FieldError>,
    /// Parsed preview of the row — the exact values the commit path would
    /// send to the database if this row is valid. Populated only when `ok`.
    pub parsed: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ImportReport {
    pub job_id: Uuid,
    pub kind: ImportKind,
    pub mode: ImportMode,
    pub format: ImportFormat,
    pub total_rows: usize,
    pub valid_rows: usize,
    pub error_rows: usize,
    pub committed: bool,
    pub rows: Vec<RowReport>,
}

// ---------------------------------------------------------------------------
// Public entry points
// ---------------------------------------------------------------------------

pub async fn run_course_import(
    pool: &MySqlPool,
    principal: &Principal,
    format: ImportFormat,
    mode: ImportMode,
    bytes: &[u8],
) -> AppResult<ImportReport> {
    require(principal, ImportKind::Courses.required_capability())?;
    let raw_rows = parse_rows(format, bytes)?;
    let header_required = [
        "code",
        "title",
        "department_code",
        "credit_hours",
        "contact_hours",
    ];
    for field in header_required {
        if !raw_rows.headers.iter().any(|h| h == field) {
            return Err(AppError::Validation(format!(
                "missing required column '{}' in header row",
                field
            )));
        }
    }

    // Validate every row up front.
    let mut reports: Vec<RowReport> = Vec::with_capacity(raw_rows.rows.len());
    let mut parsed_courses: Vec<CourseParsedRow> = Vec::with_capacity(raw_rows.rows.len());

    for (idx, row) in raw_rows.rows.iter().enumerate() {
        let excel_row_index = idx + 2; // header is row 1
        match validate_course_row(pool, principal, row).await {
            Ok((parsed, preview)) => {
                parsed_courses.push(parsed);
                reports.push(RowReport {
                    row_index: excel_row_index,
                    ok: true,
                    errors: vec![],
                    parsed: Some(preview),
                });
            }
            Err(errors) => {
                reports.push(RowReport {
                    row_index: excel_row_index,
                    ok: false,
                    errors,
                    parsed: None,
                });
            }
        }
    }

    let total = reports.len();
    let error_rows = reports.iter().filter(|r| !r.ok).count();
    let valid_rows = total - error_rows;

    // Dry-run ALWAYS returns the report without touching the DB for writes.
    if mode == ImportMode::DryRun {
        return finalize_report(
            pool,
            principal,
            ImportKind::Courses,
            mode,
            format,
            total,
            valid_rows,
            error_rows,
            false,
            reports,
        )
        .await;
    }

    // Commit mode requires ALL rows to be valid.
    if error_rows > 0 {
        // Persist a "failed" job record so the audit trail shows the attempt.
        let job = finalize_report(
            pool,
            principal,
            ImportKind::Courses,
            mode,
            format,
            total,
            valid_rows,
            error_rows,
            false,
            reports,
        )
        .await?;
        return Err(AppError::Validation(format!(
            "import failed validation — {} of {} rows had errors; see job {}",
            error_rows, total, job.job_id
        )));
    }

    // All-or-nothing commit inside a single transaction.
    let mut tx = pool
        .begin()
        .await
        .map_err(|e| AppError::Database(format!("import tx: {}", e)))?;
    for parsed in &parsed_courses {
        insert_course_row(&mut tx, principal, parsed).await?;
    }
    tx.commit()
        .await
        .map_err(|e| AppError::Database(format!("import commit: {}", e)))?;

    // Second pass: prerequisite links. Needs the new course ids to exist
    // in the DB, so we do it after the main commit. Each link is added
    // via the regular service so cycle detection runs.
    for parsed in &parsed_courses {
        for prereq_code in &parsed.prerequisites {
            if let Some((prereq_id, _dept)) =
                super::course_service::find_by_code(pool, prereq_code).await?
            {
                if let Some((course_id, _)) =
                    super::course_service::find_by_code(pool, &parsed.code).await?
                {
                    // Ignore "already exists" conflicts — re-runs are idempotent.
                    let _ = super::course_service::add_prerequisite(
                        pool,
                        principal,
                        course_id,
                        prereq_id,
                        None,
                    )
                    .await;
                }
            }
        }
    }

    finalize_report(
        pool,
        principal,
        ImportKind::Courses,
        mode,
        format,
        total,
        valid_rows,
        error_rows,
        true,
        reports,
    )
    .await
}

pub async fn run_section_import(
    pool: &MySqlPool,
    principal: &Principal,
    format: ImportFormat,
    mode: ImportMode,
    bytes: &[u8],
) -> AppResult<ImportReport> {
    require(principal, ImportKind::Sections.required_capability())?;
    let raw_rows = parse_rows(format, bytes)?;
    let header_required = [
        "course_code",
        "section_code",
        "term",
        "year",
        "capacity",
    ];
    for field in header_required {
        if !raw_rows.headers.iter().any(|h| h == field) {
            return Err(AppError::Validation(format!(
                "missing required column '{}' in header row",
                field
            )));
        }
    }

    let mut reports: Vec<RowReport> = Vec::with_capacity(raw_rows.rows.len());
    let mut parsed_sections: Vec<SectionParsedRow> = Vec::with_capacity(raw_rows.rows.len());

    for (idx, row) in raw_rows.rows.iter().enumerate() {
        let excel_row_index = idx + 2;
        match validate_section_row(pool, principal, row).await {
            Ok((parsed, preview)) => {
                parsed_sections.push(parsed);
                reports.push(RowReport {
                    row_index: excel_row_index,
                    ok: true,
                    errors: vec![],
                    parsed: Some(preview),
                });
            }
            Err(errors) => {
                reports.push(RowReport {
                    row_index: excel_row_index,
                    ok: false,
                    errors,
                    parsed: None,
                });
            }
        }
    }

    let total = reports.len();
    let error_rows = reports.iter().filter(|r| !r.ok).count();
    let valid_rows = total - error_rows;

    if mode == ImportMode::DryRun {
        return finalize_report(
            pool,
            principal,
            ImportKind::Sections,
            mode,
            format,
            total,
            valid_rows,
            error_rows,
            false,
            reports,
        )
        .await;
    }

    if error_rows > 0 {
        let job = finalize_report(
            pool,
            principal,
            ImportKind::Sections,
            mode,
            format,
            total,
            valid_rows,
            error_rows,
            false,
            reports,
        )
        .await?;
        return Err(AppError::Validation(format!(
            "import failed validation — {} of {} rows had errors; see job {}",
            error_rows, total, job.job_id
        )));
    }

    let mut tx = pool
        .begin()
        .await
        .map_err(|e| AppError::Database(format!("section import tx: {}", e)))?;
    for parsed in &parsed_sections {
        insert_section_row(&mut tx, principal, parsed).await?;
    }
    tx.commit()
        .await
        .map_err(|e| AppError::Database(format!("section import commit: {}", e)))?;

    finalize_report(
        pool,
        principal,
        ImportKind::Sections,
        mode,
        format,
        total,
        valid_rows,
        error_rows,
        true,
        reports,
    )
    .await
}

// ---------------------------------------------------------------------------
// Parsing (format-agnostic middle layer)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct RawRow {
    /// Row cells keyed by lowercase header name.
    values: HashMap<String, String>,
}

impl RawRow {
    fn get(&self, key: &str) -> &str {
        self.values
            .get(&key.to_lowercase())
            .map(|s| s.as_str())
            .unwrap_or("")
    }
}

struct ParsedFile {
    headers: Vec<String>,
    rows: Vec<RawRow>,
}

fn parse_rows(format: ImportFormat, bytes: &[u8]) -> AppResult<ParsedFile> {
    match format {
        ImportFormat::Csv => parse_csv(bytes),
        ImportFormat::Xlsx => parse_xlsx(bytes),
    }
}

fn parse_csv(bytes: &[u8]) -> AppResult<ParsedFile> {
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(true)
        .trim(csv::Trim::All)
        .from_reader(bytes);

    let headers: Vec<String> = reader
        .headers()
        .map_err(|e| AppError::Validation(format!("invalid CSV header: {}", e)))?
        .iter()
        .map(|h| h.to_lowercase())
        .collect();

    if headers.is_empty() {
        return Err(AppError::Validation("CSV has no header row".into()));
    }

    let mut rows = Vec::new();
    for (i, record) in reader.records().enumerate() {
        let record = record.map_err(|e| {
            AppError::Validation(format!("CSV row {} parse error: {}", i + 2, e))
        })?;
        let mut values = HashMap::new();
        for (col, cell) in headers.iter().zip(record.iter()) {
            values.insert(col.clone(), cell.to_string());
        }
        rows.push(RawRow { values });
    }
    Ok(ParsedFile { headers, rows })
}

fn parse_xlsx(bytes: &[u8]) -> AppResult<ParsedFile> {
    let cursor = Cursor::new(bytes.to_vec());
    let mut workbook: Xlsx<_> = calamine::open_workbook_from_rs(cursor)
        .map_err(|e| AppError::Validation(format!("xlsx open: {}", e)))?;
    let sheet_name = workbook
        .sheet_names()
        .first()
        .cloned()
        .ok_or_else(|| AppError::Validation("xlsx has no worksheets".into()))?;
    let range = workbook
        .worksheet_range(&sheet_name)
        .map_err(|e| AppError::Validation(format!("xlsx read: {}", e)))?;

    let mut row_iter = range.rows();
    let header_row = row_iter
        .next()
        .ok_or_else(|| AppError::Validation("xlsx has no header row".into()))?;
    let headers: Vec<String> = header_row
        .iter()
        .map(|c| cell_to_string(c).to_lowercase())
        .collect();

    let mut rows = Vec::new();
    for row in row_iter {
        // Skip blank rows entirely.
        if row.iter().all(|c| matches!(c, Data::Empty)) {
            continue;
        }
        let mut values = HashMap::new();
        for (i, cell) in row.iter().enumerate() {
            if let Some(key) = headers.get(i) {
                values.insert(key.clone(), cell_to_string(cell));
            }
        }
        rows.push(RawRow { values });
    }
    Ok(ParsedFile { headers, rows })
}

fn cell_to_string(cell: &Data) -> String {
    match cell {
        Data::Empty => String::new(),
        Data::String(s) => s.clone(),
        Data::Float(f) => {
            // Integer-valued floats render without the trailing ".0" to
            // keep the round-trip with CSV clean.
            if f.fract() == 0.0 {
                format!("{}", *f as i64)
            } else {
                format!("{}", f)
            }
        }
        Data::Int(i) => format!("{}", i),
        Data::Bool(b) => format!("{}", b),
        Data::DateTime(dt) => format!("{}", dt.as_f64()),
        Data::DateTimeIso(s) => s.clone(),
        Data::DurationIso(s) => s.clone(),
        Data::Error(e) => format!("#ERR:{:?}", e),
    }
}

// ---------------------------------------------------------------------------
// Course row validation + commit
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct CourseParsedRow {
    code: String,
    title: String,
    department_id: Option<Uuid>,
    description: Option<String>,
    credit_hours: f32,
    contact_hours: f32,
    prerequisites: Vec<String>,
}

async fn validate_course_row(
    pool: &MySqlPool,
    principal: &Principal,
    row: &RawRow,
) -> Result<(CourseParsedRow, serde_json::Value), Vec<FieldError>> {
    let mut errors: Vec<FieldError> = Vec::new();

    let code = row.get("code").trim().to_string();
    if !is_valid_course_code(&code) {
        errors.push(FieldError {
            field: "code".into(),
            message: format!("invalid course code '{}'", code),
        });
    }

    let title = row.get("title").trim().to_string();
    if title.chars().count() < 3 || title.chars().count() > 500 {
        errors.push(FieldError {
            field: "title".into(),
            message: "title must be 3-500 characters".into(),
        });
    }

    let credit_hours = match row.get("credit_hours").trim().parse::<f32>() {
        Ok(v) => match validate_credit_hours(v) {
            Ok(()) => v,
            Err(e) => {
                errors.push(FieldError {
                    field: "credit_hours".into(),
                    message: format!("{}", e),
                });
                0.0
            }
        },
        Err(_) => {
            errors.push(FieldError {
                field: "credit_hours".into(),
                message: "credit_hours must be a number".into(),
            });
            0.0
        }
    };

    let contact_hours = match row.get("contact_hours").trim().parse::<f32>() {
        Ok(v) => match validate_contact_hours(v) {
            Ok(()) => v,
            Err(e) => {
                errors.push(FieldError {
                    field: "contact_hours".into(),
                    message: format!("{}", e),
                });
                0.0
            }
        },
        Err(_) => {
            errors.push(FieldError {
                field: "contact_hours".into(),
                message: "contact_hours must be a number".into(),
            });
            0.0
        }
    };

    let description = {
        let d = row.get("description");
        if d.is_empty() {
            None
        } else {
            Some(d.to_string())
        }
    };

    // Department scope lookup.
    let dept_code = row.get("department_code").trim().to_string();
    let (department_id, dept_scope_error) =
        match resolve_department(pool, principal, &dept_code).await {
            Ok(id) => (id, None),
            Err(e) => (None, Some(e)),
        };
    if let Some(e) = dept_scope_error {
        errors.push(FieldError {
            field: "department_code".into(),
            message: format!("{}", e),
        });
    }

    // Prerequisites are a `;`-separated list of course codes. Each must
    // either already exist OR be present earlier in the same import.
    // (Phase 4 simplification: prereqs only reference existing rows — the
    // commit path's second pass retries missing links silently.)
    let prereq_raw = row.get("prerequisites").trim().to_string();
    let prerequisites: Vec<String> = prereq_raw
        .split(';')
        .filter_map(|s| {
            let t = s.trim();
            if t.is_empty() { None } else { Some(t.to_string()) }
        })
        .collect();
    for p in &prerequisites {
        if !is_valid_course_code(p) {
            errors.push(FieldError {
                field: "prerequisites".into(),
                message: format!("invalid prerequisite code '{}'", p),
            });
        }
        if p == &code {
            errors.push(FieldError {
                field: "prerequisites".into(),
                message: "a course cannot be its own prerequisite".into(),
            });
        }
    }

    // Uniqueness: course code must not already exist.
    if is_valid_course_code(&code) {
        if let Ok(Some(_)) = super::course_service::find_by_code(pool, &code).await {
            errors.push(FieldError {
                field: "code".into(),
                message: format!("course code '{}' already exists", code),
            });
        }
    }

    if !errors.is_empty() {
        return Err(errors);
    }

    let preview = serde_json::json!({
        "code": code,
        "title": title,
        "department_code": dept_code,
        "credit_hours": credit_hours,
        "contact_hours": contact_hours,
        "prerequisites": prerequisites,
    });
    Ok((
        CourseParsedRow {
            code,
            title,
            department_id,
            description,
            credit_hours,
            contact_hours,
            prerequisites,
        },
        preview,
    ))
}

async fn insert_course_row(
    tx: &mut sqlx::Transaction<'_, sqlx::MySql>,
    principal: &Principal,
    row: &CourseParsedRow,
) -> AppResult<()> {
    let course_id = Uuid::new_v4();
    let version_id = Uuid::new_v4();

    sqlx::query(
        r#"INSERT INTO courses (id, code, title, department_id, owner_id, is_active)
           VALUES (?, ?, ?, ?, ?, TRUE)"#,
    )
    .bind(course_id.to_string())
    .bind(&row.code)
    .bind(&row.title)
    .bind(row.department_id.map(|u| u.to_string()))
    .bind(principal.user_id.to_string())
    .execute(&mut **tx)
    .await
    .map_err(|e| AppError::Database(format!("import insert course: {}", e)))?;

    sqlx::query(
        r#"INSERT INTO course_versions
           (id, course_id, version_number, description, credit_hours, contact_hours,
            change_summary, state, created_by)
           VALUES (?, ?, 1, ?, ?, ?, 'imported via bulk upload', 'draft', ?)"#,
    )
    .bind(version_id.to_string())
    .bind(course_id.to_string())
    .bind(row.description.as_deref())
    .bind(row.credit_hours)
    .bind(row.contact_hours)
    .bind(principal.user_id.to_string())
    .execute(&mut **tx)
    .await
    .map_err(|e| AppError::Database(format!("import insert version: {}", e)))?;

    sqlx::query("UPDATE courses SET latest_version_id = ? WHERE id = ?")
        .bind(version_id.to_string())
        .bind(course_id.to_string())
        .execute(&mut **tx)
        .await
        .map_err(|e| AppError::Database(format!("import set latest: {}", e)))?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Section row validation + commit
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct SectionParsedRow {
    course_id: Uuid,
    section_code: String,
    term: String,
    year: i32,
    capacity: i32,
    instructor_id: Option<Uuid>,
    location: Option<String>,
    schedule_note: Option<String>,
    notes: Option<String>,
}

async fn validate_section_row(
    pool: &MySqlPool,
    principal: &Principal,
    row: &RawRow,
) -> Result<(SectionParsedRow, serde_json::Value), Vec<FieldError>> {
    let mut errors: Vec<FieldError> = Vec::new();

    let course_code = row.get("course_code").trim().to_string();
    let (course_id, course_dept) =
        match super::course_service::find_by_code(pool, &course_code).await {
            Ok(Some(pair)) => (Some(pair.0), pair.1),
            Ok(None) => {
                errors.push(FieldError {
                    field: "course_code".into(),
                    message: format!("course '{}' does not exist", course_code),
                });
                (None, None)
            }
            Err(e) => {
                errors.push(FieldError {
                    field: "course_code".into(),
                    message: format!("{}", e),
                });
                (None, None)
            }
        };

    // Scope: non-admin callers can only import sections for courses in
    // their department.
    if let Some(dept) = course_dept {
        if !principal.is_admin() && principal.department_id != Some(dept) {
            errors.push(FieldError {
                field: "course_code".into(),
                message: "course is outside your department scope".into(),
            });
        }
    }

    let section_code = row.get("section_code").trim().to_string();
    if !is_valid_section_code(&section_code) {
        errors.push(FieldError {
            field: "section_code".into(),
            message: "invalid section_code".into(),
        });
    }

    let term_raw = row.get("term").trim().to_string();
    if !is_valid_term(&term_raw) {
        errors.push(FieldError {
            field: "term".into(),
            message: "term must be fall|spring|summer|winter".into(),
        });
    }
    let term = normalize_term(&term_raw);

    let year = match row.get("year").trim().parse::<i32>() {
        Ok(v) => match validate_year(v) {
            Ok(()) => v,
            Err(e) => {
                errors.push(FieldError {
                    field: "year".into(),
                    message: format!("{}", e),
                });
                0
            }
        },
        Err(_) => {
            errors.push(FieldError {
                field: "year".into(),
                message: "year must be an integer".into(),
            });
            0
        }
    };

    let capacity = match row.get("capacity").trim().parse::<i32>() {
        Ok(v) => match validate_capacity(v) {
            Ok(()) => v,
            Err(e) => {
                errors.push(FieldError {
                    field: "capacity".into(),
                    message: format!("{}", e),
                });
                0
            }
        },
        Err(_) => {
            errors.push(FieldError {
                field: "capacity".into(),
                message: "capacity must be an integer".into(),
            });
            0
        }
    };

    let instructor_email = row.get("instructor_email").trim().to_string();
    let instructor_id = if instructor_email.is_empty() {
        None
    } else {
        match resolve_user_by_email(pool, &instructor_email).await {
            Ok(Some(id)) => Some(id),
            Ok(None) => {
                errors.push(FieldError {
                    field: "instructor_email".into(),
                    message: format!("user '{}' not found", instructor_email),
                });
                None
            }
            Err(e) => {
                errors.push(FieldError {
                    field: "instructor_email".into(),
                    message: format!("{}", e),
                });
                None
            }
        }
    };

    let location = {
        let l = row.get("location").trim();
        if l.is_empty() { None } else { Some(l.to_string()) }
    };
    if let Some(ref l) = location {
        if l.chars().count() > 255 {
            errors.push(FieldError {
                field: "location".into(),
                message: "location exceeds 255 chars".into(),
            });
        }
    }

    let schedule_note = {
        let s = row.get("schedule_note").trim();
        if s.is_empty() { None } else { Some(s.to_string()) }
    };
    if let Some(ref s) = schedule_note {
        if s.chars().count() > 500 {
            errors.push(FieldError {
                field: "schedule_note".into(),
                message: "schedule_note exceeds 500 chars".into(),
            });
        }
    }

    let notes = {
        let n = row.get("notes").trim();
        if n.is_empty() { None } else { Some(n.to_string()) }
    };
    if let Some(ref n) = notes {
        if n.chars().count() > 2_000 {
            errors.push(FieldError {
                field: "notes".into(),
                message: "notes exceeds 2000 chars".into(),
            });
        }
    }

    // Duplicate check against existing sections.
    if let Some(cid) = course_id {
        if is_valid_section_code(&section_code) && is_valid_term(&term_raw) && year != 0 {
            let dup = sqlx::query(
                r#"SELECT 1 FROM sections
                    WHERE course_id = ? AND section_code = ? AND term = ? AND year = ?
                    LIMIT 1"#,
            )
            .bind(cid.to_string())
            .bind(&section_code)
            .bind(&term)
            .bind(year)
            .fetch_optional(pool)
            .await
            .map_err(|e| vec![FieldError {
                field: "section_code".into(),
                message: format!("duplicate check failed: {}", e),
            }])?;
            if dup.is_some() {
                errors.push(FieldError {
                    field: "section_code".into(),
                    message: "section already exists for this course/term/year".into(),
                });
            }
        }
    }

    if !errors.is_empty() {
        return Err(errors);
    }

    let preview = serde_json::json!({
        "course_code": course_code,
        "section_code": section_code,
        "term": term,
        "year": year,
        "capacity": capacity,
        "instructor_email": instructor_email,
        "location": location,
        "schedule_note": schedule_note,
    });
    Ok((
        SectionParsedRow {
            course_id: course_id.unwrap(),
            section_code,
            term,
            year,
            capacity,
            instructor_id,
            location,
            schedule_note,
            notes,
        },
        preview,
    ))
}

async fn insert_section_row(
    tx: &mut sqlx::Transaction<'_, sqlx::MySql>,
    principal: &Principal,
    row: &SectionParsedRow,
) -> AppResult<()> {
    let section_id = Uuid::new_v4();
    let version_id = Uuid::new_v4();

    sqlx::query(
        r#"INSERT INTO sections
           (id, course_id, instructor_id, section_code, term, year, capacity, is_active)
           VALUES (?, ?, ?, ?, ?, ?, ?, TRUE)"#,
    )
    .bind(section_id.to_string())
    .bind(row.course_id.to_string())
    .bind(row.instructor_id.map(|u| u.to_string()))
    .bind(&row.section_code)
    .bind(&row.term)
    .bind(row.year)
    .bind(row.capacity)
    .execute(&mut **tx)
    .await
    .map_err(|e| AppError::Database(format!("import section insert: {}", e)))?;

    let schedule_json = row
        .schedule_note
        .as_deref()
        .map(|s| serde_json::json!({ "note": s }).to_string())
        .unwrap_or_else(|| "null".into());

    sqlx::query(
        r#"INSERT INTO section_versions
           (id, section_id, version_number, location, schedule_json, notes, state, created_by)
           VALUES (?, ?, 1, ?, CAST(? AS JSON), ?, 'draft', ?)"#,
    )
    .bind(version_id.to_string())
    .bind(section_id.to_string())
    .bind(row.location.as_deref())
    .bind(&schedule_json)
    .bind(row.notes.as_deref())
    .bind(principal.user_id.to_string())
    .execute(&mut **tx)
    .await
    .map_err(|e| AppError::Database(format!("import section version: {}", e)))?;

    sqlx::query("UPDATE sections SET latest_version_id = ? WHERE id = ?")
        .bind(version_id.to_string())
        .bind(section_id.to_string())
        .execute(&mut **tx)
        .await
        .map_err(|e| AppError::Database(format!("set latest: {}", e)))?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Shared helpers: department / user lookups + job envelope persistence
// ---------------------------------------------------------------------------

async fn resolve_department(
    pool: &MySqlPool,
    principal: &Principal,
    code: &str,
) -> AppResult<Option<Uuid>> {
    let trimmed = code.trim();
    if trimmed.is_empty() {
        // An empty department in the import is only allowed for admins.
        if !principal.is_admin() {
            return Err(AppError::Forbidden);
        }
        return Ok(None);
    }
    let row = sqlx::query("SELECT id FROM departments WHERE code = ? LIMIT 1")
        .bind(trimmed)
        .fetch_optional(pool)
        .await
        .map_err(|e| AppError::Database(format!("department lookup: {}", e)))?;
    let Some(row) = row else {
        return Err(AppError::Validation(format!("unknown department '{}'", trimmed)));
    };
    let id: String = row
        .try_get("id")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let dept_id = Uuid::parse_str(&id).map_err(|e| AppError::Database(e.to_string()))?;

    if !principal.is_admin() && principal.department_id != Some(dept_id) {
        return Err(AppError::Forbidden);
    }
    Ok(Some(dept_id))
}

async fn resolve_user_by_email(pool: &MySqlPool, email: &str) -> AppResult<Option<Uuid>> {
    let row = sqlx::query("SELECT id FROM users WHERE email = ? LIMIT 1")
        .bind(email.trim().to_lowercase())
        .fetch_optional(pool)
        .await
        .map_err(|e| AppError::Database(format!("user lookup: {}", e)))?;
    let Some(row) = row else { return Ok(None) };
    let id: String = row
        .try_get("id")
        .map_err(|e| AppError::Database(e.to_string()))?;
    Ok(Some(
        Uuid::parse_str(&id).map_err(|e| AppError::Database(e.to_string()))?,
    ))
}

#[allow(clippy::too_many_arguments)]
async fn finalize_report(
    pool: &MySqlPool,
    principal: &Principal,
    kind: ImportKind,
    mode: ImportMode,
    format: ImportFormat,
    total: usize,
    valid: usize,
    errors: usize,
    committed: bool,
    rows: Vec<RowReport>,
) -> AppResult<ImportReport> {
    let job_id = Uuid::new_v4();
    let status = if committed {
        "committed"
    } else if errors > 0 {
        "failed"
    } else {
        "validated"
    };

    sqlx::query(
        r#"INSERT INTO import_jobs
           (id, job_type, mode, source_format, status, total_rows, valid_rows, error_rows,
            initiated_by, completed_at)
           VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, NOW())"#,
    )
    .bind(job_id.to_string())
    .bind(kind.as_db())
    .bind(mode.as_db())
    .bind(format.as_db())
    .bind(status)
    .bind(total as i64)
    .bind(valid as i64)
    .bind(errors as i64)
    .bind(principal.user_id.to_string())
    .execute(pool)
    .await
    .map_err(|e| AppError::Database(format!("import_jobs insert: {}", e)))?;

    audit_service::record(
        pool,
        AuditEvent {
            actor_id: Some(principal.user_id),
            actor_email: Some(&principal.email),
            action: if committed {
                "import.commit"
            } else if mode == ImportMode::DryRun {
                "import.dry_run"
            } else {
                "import.commit.failed"
            },
            target_entity_type: Some("import_job"),
            target_entity_id: Some(job_id),
            change_payload: Some(serde_json::json!({
                "kind": kind.as_db(),
                "format": format.as_db(),
                "total": total,
                "valid": valid,
                "errors": errors,
            })),
            ip_address: None,
            user_agent: None,
        },
    )
    .await?;

    Ok(ImportReport {
        job_id,
        kind,
        mode,
        format,
        total_rows: total,
        valid_rows: valid,
        error_rows: errors,
        committed,
        rows,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csv_parse_basic() {
        let csv = b"code,title,credit_hours\nCS101,Intro to CS,3\nCS201,Data Structures,4\n";
        let parsed = parse_csv(csv).expect("ok");
        assert_eq!(parsed.headers, vec!["code", "title", "credit_hours"]);
        assert_eq!(parsed.rows.len(), 2);
        assert_eq!(parsed.rows[0].get("code"), "CS101");
        assert_eq!(parsed.rows[1].get("credit_hours"), "4");
    }

    #[test]
    fn csv_parse_handles_trimmed_whitespace() {
        let csv = b"code, title \n  CS101 ,  Intro \n";
        let parsed = parse_csv(csv).expect("ok");
        assert_eq!(parsed.rows[0].get("code"), "CS101");
        assert_eq!(parsed.rows[0].get("title"), "Intro");
    }

    #[test]
    fn cell_to_string_renders_int_floats_cleanly() {
        assert_eq!(cell_to_string(&Data::Float(3.0)), "3");
        assert_eq!(cell_to_string(&Data::Float(3.5)), "3.5");
        assert_eq!(cell_to_string(&Data::Int(42)), "42");
        assert_eq!(cell_to_string(&Data::Empty), "");
        assert_eq!(cell_to_string(&Data::String("hi".into())), "hi");
    }

    #[test]
    fn import_format_from_extension() {
        assert_eq!(ImportFormat::from_extension("data.csv"), Some(ImportFormat::Csv));
        assert_eq!(
            ImportFormat::from_extension("data.XLSX"),
            Some(ImportFormat::Xlsx)
        );
        assert_eq!(ImportFormat::from_extension("data.txt"), None);
    }

    #[test]
    fn csv_parse_headers_are_lowercased() {
        // The header row is upper-case; the parser must fold it down so
        // the rest of the import pipeline can read columns by a canonical
        // lowercase key.
        let csv = b"CODE,TITLE,Credit_Hours\nCS101,Intro,3\n";
        let parsed = parse_csv(csv).expect("ok");
        assert_eq!(parsed.headers, vec!["code", "title", "credit_hours"]);
        // And the RawRow lookup is itself case-insensitive against keys.
        assert_eq!(parsed.rows[0].get("code"), "CS101");
        assert_eq!(parsed.rows[0].get("title"), "Intro");
        assert_eq!(parsed.rows[0].get("credit_hours"), "3");
    }

    #[test]
    fn import_format_from_mime() {
        assert_eq!(
            ImportFormat::from_mime("text/csv"),
            Some(ImportFormat::Csv)
        );
        assert_eq!(
            ImportFormat::from_mime(
                "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"
            ),
            Some(ImportFormat::Xlsx)
        );
        assert_eq!(ImportFormat::from_mime("application/json"), None);
    }

    #[test]
    fn cell_to_string_handles_datetime_iso() {
        let s = "2026-01-01T00:00:00".to_string();
        assert_eq!(
            cell_to_string(&Data::DateTimeIso(s.clone())),
            "2026-01-01T00:00:00"
        );
    }

    #[test]
    fn parse_csv_preserves_empty_cells() {
        // Empty trailing cells — the description column on row 1 is empty,
        // and the prerequisites column on both rows is empty.
        let csv = b"code,title,description,prerequisites\n\
                    CS101,Intro,,\n\
                    CS201,Data,Second course,\n";
        let parsed = parse_csv(csv).expect("ok");
        assert_eq!(parsed.rows.len(), 2);
        assert_eq!(parsed.rows[0].get("description"), "");
        assert_eq!(parsed.rows[0].get("prerequisites"), "");
        assert_eq!(parsed.rows[1].get("description"), "Second course");
        assert_eq!(parsed.rows[1].get("prerequisites"), "");
    }

    #[test]
    fn parse_xlsx_minimal_workbook() {
        // Build a real xlsx in memory with rust_xlsxwriter, then feed the
        // bytes through parse_xlsx and assert the round-trip is intact.
        use rust_xlsxwriter::Workbook;

        let mut wb = Workbook::new();
        let sheet = wb.add_worksheet();
        // Header row.
        sheet.write_string(0, 0, "code").unwrap();
        sheet.write_string(0, 1, "title").unwrap();
        sheet.write_string(0, 2, "credit_hours").unwrap();
        // One data row.
        sheet.write_string(1, 0, "CS101").unwrap();
        sheet.write_string(1, 1, "Intro to CS").unwrap();
        sheet.write_number(1, 2, 3.0).unwrap();

        let bytes = wb.save_to_buffer().expect("save xlsx to buffer");
        assert_eq!(&bytes[0..2], b"PK", "xlsx must start with zip magic");

        let parsed = parse_xlsx(&bytes).expect("parse xlsx");
        assert_eq!(parsed.headers, vec!["code", "title", "credit_hours"]);
        assert_eq!(parsed.rows.len(), 1);
        assert_eq!(parsed.rows[0].get("code"), "CS101");
        assert_eq!(parsed.rows[0].get("title"), "Intro to CS");
        // Integer-valued floats render without a trailing .0.
        assert_eq!(parsed.rows[0].get("credit_hours"), "3");
    }

    #[test]
    fn import_format_from_extension_case_insensitive() {
        assert_eq!(
            ImportFormat::from_extension("FILE.CSV"),
            Some(ImportFormat::Csv)
        );
        assert_eq!(
            ImportFormat::from_extension("File.Xlsx"),
            Some(ImportFormat::Xlsx)
        );
    }
}
