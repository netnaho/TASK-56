//! Bulk export — CSV and XLSX writers for courses and sections.
//!
//! Every export is **department-scoped**:
//!
//! * Admin sees every row.
//! * Librarian sees every row (library-wide read).
//! * Everyone else sees only the rows belonging to their own
//!   `department_id`.
//!
//! Both formats use the same column order so the output of either can be
//! re-imported without re-mapping.
//!
//! Templates — an "empty" CSV / XLSX with just the header row — are
//! available via [`course_template`] and [`section_template`] so
//! operators can download a starting point for the import pipeline.

use rust_xlsxwriter::Workbook;
use sqlx::{MySqlPool, Row};
use uuid::Uuid;

use super::audit_service::{self, AuditEvent};
use super::authorization::{require, Capability};
use super::import_service::ImportFormat;
use super::principal::{Principal, Role};
use crate::errors::{AppError, AppResult};

// ---------------------------------------------------------------------------
// Column headers (keep in sync with import_service::run_*_import)
// ---------------------------------------------------------------------------

pub const COURSE_COLUMNS: &[&str] = &[
    "code",
    "title",
    "department_code",
    "credit_hours",
    "contact_hours",
    "description",
    "prerequisites",
];

pub const SECTION_COLUMNS: &[&str] = &[
    "course_code",
    "section_code",
    "term",
    "year",
    "capacity",
    "instructor_email",
    "location",
    "schedule_note",
    "notes",
];

/// MIME types returned by [`export_courses`] / [`export_sections`].
pub fn mime_for(format: ImportFormat) -> &'static str {
    match format {
        ImportFormat::Csv => "text/csv",
        ImportFormat::Xlsx => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
    }
}

pub fn suggested_filename(entity: &str, format: ImportFormat) -> String {
    match format {
        ImportFormat::Csv => format!("{}.csv", entity),
        ImportFormat::Xlsx => format!("{}.xlsx", entity),
    }
}

// ---------------------------------------------------------------------------
// Templates (header row only)
// ---------------------------------------------------------------------------

pub fn course_template(format: ImportFormat) -> AppResult<Vec<u8>> {
    write_header_only(COURSE_COLUMNS, format)
}

pub fn section_template(format: ImportFormat) -> AppResult<Vec<u8>> {
    write_header_only(SECTION_COLUMNS, format)
}

fn write_header_only(columns: &[&str], format: ImportFormat) -> AppResult<Vec<u8>> {
    match format {
        ImportFormat::Csv => {
            let mut wtr = csv::Writer::from_writer(Vec::new());
            wtr.write_record(columns)
                .map_err(|e| AppError::Internal(format!("csv header: {}", e)))?;
            wtr.flush()
                .map_err(|e| AppError::Internal(format!("csv flush: {}", e)))?;
            wtr.into_inner()
                .map_err(|e| AppError::Internal(format!("csv buffer: {}", e)))
        }
        ImportFormat::Xlsx => {
            let mut wb = Workbook::new();
            let sheet = wb.add_worksheet();
            for (col_idx, name) in columns.iter().enumerate() {
                sheet
                    .write_string(0, col_idx as u16, *name)
                    .map_err(|e| AppError::Internal(format!("xlsx write: {}", e)))?;
            }
            wb.save_to_buffer()
                .map_err(|e| AppError::Internal(format!("xlsx save: {}", e)))
        }
    }
}

// ---------------------------------------------------------------------------
// Course export
// ---------------------------------------------------------------------------

pub async fn export_courses(
    pool: &MySqlPool,
    principal: &Principal,
    format: ImportFormat,
) -> AppResult<Vec<u8>> {
    require(principal, Capability::ExportCourses)?;
    let rows = fetch_course_rows(pool, principal).await?;
    let bytes = match format {
        ImportFormat::Csv => write_courses_csv(&rows)?,
        ImportFormat::Xlsx => write_courses_xlsx(&rows)?,
    };

    audit_service::record(
        pool,
        AuditEvent {
            actor_id: Some(principal.user_id),
            actor_email: Some(&principal.email),
            action: "export.courses",
            target_entity_type: Some("course"),
            target_entity_id: None,
            change_payload: Some(serde_json::json!({
                "format": format,
                "row_count": rows.len(),
                "department_scope": principal.department_id,
            })),
            ip_address: None,
            user_agent: None,
        },
    )
    .await?;
    Ok(bytes)
}

pub async fn export_sections(
    pool: &MySqlPool,
    principal: &Principal,
    format: ImportFormat,
) -> AppResult<Vec<u8>> {
    require(principal, Capability::ExportSections)?;
    let rows = fetch_section_rows(pool, principal).await?;
    let bytes = match format {
        ImportFormat::Csv => write_sections_csv(&rows)?,
        ImportFormat::Xlsx => write_sections_xlsx(&rows)?,
    };

    audit_service::record(
        pool,
        AuditEvent {
            actor_id: Some(principal.user_id),
            actor_email: Some(&principal.email),
            action: "export.sections",
            target_entity_type: Some("section"),
            target_entity_id: None,
            change_payload: Some(serde_json::json!({
                "format": format,
                "row_count": rows.len(),
                "department_scope": principal.department_id,
            })),
            ip_address: None,
            user_agent: None,
        },
    )
    .await?;
    Ok(bytes)
}

// ---------------------------------------------------------------------------
// Fetchers — enforce scope IN SQL, not after the fact.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct CourseRowFlat {
    code: String,
    title: String,
    department_code: String,
    credit_hours: f32,
    contact_hours: f32,
    description: String,
    prerequisites: String,
}

#[derive(Debug, Clone)]
struct SectionRowFlat {
    course_code: String,
    section_code: String,
    term: String,
    year: i32,
    capacity: i32,
    instructor_email: String,
    location: String,
    schedule_note: String,
    notes: String,
}

fn scope_department(principal: &Principal) -> Option<Uuid> {
    if principal.is_admin() || principal.has_role(Role::Librarian) {
        None
    } else {
        // Everyone else is pinned to their own department. No bypass.
        principal.department_id
    }
}

async fn fetch_course_rows(
    pool: &MySqlPool,
    principal: &Principal,
) -> AppResult<Vec<CourseRowFlat>> {
    let scope = scope_department(principal);

    let mut sql = String::from(
        r#"SELECT c.id, c.code, c.title, c.current_version_id,
                  d.code AS department_code,
                  cv.description, cv.credit_hours, cv.contact_hours
             FROM courses c
        LEFT JOIN departments d ON d.id = c.department_id
        LEFT JOIN course_versions cv ON cv.id = c.current_version_id
            WHERE 1=1"#,
    );
    if scope.is_some() {
        sql.push_str(" AND c.department_id = ?");
    }
    sql.push_str(" ORDER BY c.code ASC");

    let mut q = sqlx::query(&sql);
    if let Some(d) = scope {
        q = q.bind(d.to_string());
    }
    let rows = q
        .fetch_all(pool)
        .await
        .map_err(|e| AppError::Database(format!("export_courses fetch: {}", e)))?;

    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let id: String = row.try_get("id").map_err(|e| AppError::Database(e.to_string()))?;
        let code: String = row
            .try_get("code")
            .map_err(|e| AppError::Database(e.to_string()))?;
        let title: String = row
            .try_get("title")
            .map_err(|e| AppError::Database(e.to_string()))?;
        let department_code: Option<String> = row
            .try_get("department_code")
            .map_err(|e| AppError::Database(e.to_string()))?;
        let description: Option<String> = row
            .try_get("description")
            .map_err(|e| AppError::Database(e.to_string()))?;
        let credit_hours: Option<f32> = row
            .try_get("credit_hours")
            .map_err(|e| AppError::Database(e.to_string()))?;
        let contact_hours: Option<f32> = row
            .try_get("contact_hours")
            .map_err(|e| AppError::Database(e.to_string()))?;

        // Prereqs as a `;`-separated list of codes.
        let uid = Uuid::parse_str(&id).map_err(|e| AppError::Database(e.to_string()))?;
        let prereq_rows = sqlx::query(
            r#"SELECT c.code FROM course_prerequisites p
                 JOIN courses c ON c.id = p.prerequisite_course_id
                WHERE p.course_id = ? ORDER BY c.code ASC"#,
        )
        .bind(uid.to_string())
        .fetch_all(pool)
        .await
        .map_err(|e| AppError::Database(format!("export prereqs: {}", e)))?;
        let mut prereqs: Vec<String> = Vec::new();
        for pr in prereq_rows {
            let code: String = pr
                .try_get("code")
                .map_err(|e| AppError::Database(e.to_string()))?;
            prereqs.push(code);
        }

        out.push(CourseRowFlat {
            code,
            title,
            department_code: department_code.unwrap_or_default(),
            credit_hours: credit_hours.unwrap_or(0.0),
            contact_hours: contact_hours.unwrap_or(0.0),
            description: description.unwrap_or_default(),
            prerequisites: prereqs.join(";"),
        });
    }
    Ok(out)
}

async fn fetch_section_rows(
    pool: &MySqlPool,
    principal: &Principal,
) -> AppResult<Vec<SectionRowFlat>> {
    let scope = scope_department(principal);

    let mut sql = String::from(
        r#"SELECT c.code AS course_code, s.section_code, s.term, s.year,
                  s.capacity,
                  u.email AS instructor_email,
                  sv.location,
                  CAST(sv.schedule_json AS CHAR) AS schedule_text,
                  sv.notes
             FROM sections s
             JOIN courses c ON c.id = s.course_id
        LEFT JOIN users u ON u.id = s.instructor_id
        LEFT JOIN section_versions sv ON sv.id = s.current_version_id
            WHERE 1=1"#,
    );
    if scope.is_some() {
        sql.push_str(" AND c.department_id = ?");
    }
    sql.push_str(" ORDER BY s.year DESC, s.term ASC, c.code ASC, s.section_code ASC");

    let mut q = sqlx::query(&sql);
    if let Some(d) = scope {
        q = q.bind(d.to_string());
    }
    let rows = q
        .fetch_all(pool)
        .await
        .map_err(|e| AppError::Database(format!("export_sections fetch: {}", e)))?;

    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let course_code: String = row
            .try_get("course_code")
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
        let instructor_email: Option<String> = row
            .try_get("instructor_email")
            .map_err(|e| AppError::Database(e.to_string()))?;
        let location: Option<String> = row
            .try_get("location")
            .map_err(|e| AppError::Database(e.to_string()))?;
        let schedule_text: Option<String> = row
            .try_get("schedule_text")
            .map_err(|e| AppError::Database(e.to_string()))?;
        let notes: Option<String> = row
            .try_get("notes")
            .map_err(|e| AppError::Database(e.to_string()))?;

        let schedule_note = schedule_text
            .as_deref()
            .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok())
            .and_then(|v| v.get("note").and_then(|n| n.as_str().map(String::from)))
            .unwrap_or_default();

        out.push(SectionRowFlat {
            course_code,
            section_code,
            term,
            year,
            capacity: capacity.unwrap_or(0),
            instructor_email: instructor_email.unwrap_or_default(),
            location: location.unwrap_or_default(),
            schedule_note,
            notes: notes.unwrap_or_default(),
        });
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Writers
// ---------------------------------------------------------------------------

fn write_courses_csv(rows: &[CourseRowFlat]) -> AppResult<Vec<u8>> {
    let mut wtr = csv::Writer::from_writer(Vec::new());
    wtr.write_record(COURSE_COLUMNS)
        .map_err(|e| AppError::Internal(format!("csv header: {}", e)))?;
    for r in rows {
        wtr.write_record([
            r.code.as_str(),
            r.title.as_str(),
            r.department_code.as_str(),
            &format!("{}", r.credit_hours),
            &format!("{}", r.contact_hours),
            r.description.as_str(),
            r.prerequisites.as_str(),
        ])
        .map_err(|e| AppError::Internal(format!("csv row: {}", e)))?;
    }
    wtr.flush()
        .map_err(|e| AppError::Internal(format!("csv flush: {}", e)))?;
    wtr.into_inner()
        .map_err(|e| AppError::Internal(format!("csv buffer: {}", e)))
}

fn write_courses_xlsx(rows: &[CourseRowFlat]) -> AppResult<Vec<u8>> {
    let mut wb = Workbook::new();
    let sheet = wb.add_worksheet();
    for (col, name) in COURSE_COLUMNS.iter().enumerate() {
        sheet
            .write_string(0, col as u16, *name)
            .map_err(|e| AppError::Internal(format!("xlsx write: {}", e)))?;
    }
    for (i, r) in rows.iter().enumerate() {
        let row = (i + 1) as u32;
        sheet
            .write_string(row, 0, &r.code)
            .and_then(|s| s.write_string(row, 1, &r.title))
            .and_then(|s| s.write_string(row, 2, &r.department_code))
            .and_then(|s| s.write_number(row, 3, r.credit_hours as f64))
            .and_then(|s| s.write_number(row, 4, r.contact_hours as f64))
            .and_then(|s| s.write_string(row, 5, &r.description))
            .and_then(|s| s.write_string(row, 6, &r.prerequisites))
            .map_err(|e| AppError::Internal(format!("xlsx row: {}", e)))?;
    }
    wb.save_to_buffer()
        .map_err(|e| AppError::Internal(format!("xlsx save: {}", e)))
}

fn write_sections_csv(rows: &[SectionRowFlat]) -> AppResult<Vec<u8>> {
    let mut wtr = csv::Writer::from_writer(Vec::new());
    wtr.write_record(SECTION_COLUMNS)
        .map_err(|e| AppError::Internal(format!("csv header: {}", e)))?;
    for r in rows {
        wtr.write_record([
            r.course_code.as_str(),
            r.section_code.as_str(),
            r.term.as_str(),
            &format!("{}", r.year),
            &format!("{}", r.capacity),
            r.instructor_email.as_str(),
            r.location.as_str(),
            r.schedule_note.as_str(),
            r.notes.as_str(),
        ])
        .map_err(|e| AppError::Internal(format!("csv row: {}", e)))?;
    }
    wtr.flush()
        .map_err(|e| AppError::Internal(format!("csv flush: {}", e)))?;
    wtr.into_inner()
        .map_err(|e| AppError::Internal(format!("csv buffer: {}", e)))
}

fn write_sections_xlsx(rows: &[SectionRowFlat]) -> AppResult<Vec<u8>> {
    let mut wb = Workbook::new();
    let sheet = wb.add_worksheet();
    for (col, name) in SECTION_COLUMNS.iter().enumerate() {
        sheet
            .write_string(0, col as u16, *name)
            .map_err(|e| AppError::Internal(format!("xlsx write: {}", e)))?;
    }
    for (i, r) in rows.iter().enumerate() {
        let row = (i + 1) as u32;
        sheet
            .write_string(row, 0, &r.course_code)
            .and_then(|s| s.write_string(row, 1, &r.section_code))
            .and_then(|s| s.write_string(row, 2, &r.term))
            .and_then(|s| s.write_number(row, 3, r.year as f64))
            .and_then(|s| s.write_number(row, 4, r.capacity as f64))
            .and_then(|s| s.write_string(row, 5, &r.instructor_email))
            .and_then(|s| s.write_string(row, 6, &r.location))
            .and_then(|s| s.write_string(row, 7, &r.schedule_note))
            .and_then(|s| s.write_string(row, 8, &r.notes))
            .map_err(|e| AppError::Internal(format!("xlsx row: {}", e)))?;
    }
    wb.save_to_buffer()
        .map_err(|e| AppError::Internal(format!("xlsx save: {}", e)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csv_template_has_header_only() {
        let bytes = course_template(ImportFormat::Csv).expect("csv template");
        let s = String::from_utf8(bytes).unwrap();
        // First line is the header, and there is no second data line.
        assert!(s.starts_with("code,title,department_code"));
        let line_count = s.lines().count();
        assert_eq!(line_count, 1, "template must have header only: got {}", s);
    }

    #[test]
    fn xlsx_template_is_real_xlsx() {
        let bytes = course_template(ImportFormat::Xlsx).expect("xlsx template");
        // xlsx files are ZIP archives; they always start with "PK".
        assert_eq!(&bytes[..2], b"PK", "xlsx must start with ZIP magic bytes");
        // And must not be a CSV dressed up as xlsx.
        assert!(bytes.len() > 100, "xlsx is suspiciously small: {} bytes", bytes.len());
    }

    #[test]
    fn course_columns_match_import_contract() {
        // Keep the export column order in lockstep with the importer —
        // round-tripping a non-empty export must produce a valid import.
        assert_eq!(
            COURSE_COLUMNS,
            &["code", "title", "department_code", "credit_hours",
              "contact_hours", "description", "prerequisites"]
        );
    }

    #[test]
    fn section_columns_match_import_contract() {
        assert_eq!(
            SECTION_COLUMNS,
            &["course_code", "section_code", "term", "year", "capacity",
              "instructor_email", "location", "schedule_note", "notes"]
        );
    }

    #[test]
    fn xlsx_template_bytes_are_nontrivial_zip() {
        // The section template must be a real xlsx: zip magic at the
        // front, and long enough that it can't be a degenerate CSV
        // dressed up with the wrong extension.
        let bytes = section_template(ImportFormat::Xlsx).expect("xlsx template");
        assert_eq!(&bytes[..2], b"PK", "xlsx must start with ZIP magic");
        assert!(
            bytes.len() > 1024,
            "xlsx template should be > 1024 bytes, got {}",
            bytes.len()
        );
    }

    #[test]
    fn csv_template_has_exactly_one_line() {
        for (label, bytes) in [
            ("courses", course_template(ImportFormat::Csv).expect("csv courses")),
            ("sections", section_template(ImportFormat::Csv).expect("csv sections")),
        ] {
            let s = String::from_utf8(bytes).expect("utf8");
            let nonempty: Vec<&str> = s.split('\n').filter(|l| !l.is_empty()).collect();
            assert_eq!(
                nonempty.len(),
                1,
                "{} csv template must contain exactly one non-empty line, got {:?}",
                label,
                nonempty
            );
        }
    }

    #[test]
    fn columns_match_import_contract() {
        // An explicit, load-bearing assertion on the exact contract lists
        // used by the import and export pipelines. Any reordering here is
        // a breaking change to the round-trip format.
        assert_eq!(
            COURSE_COLUMNS,
            &[
                "code",
                "title",
                "department_code",
                "credit_hours",
                "contact_hours",
                "description",
                "prerequisites",
            ]
        );
        assert_eq!(
            SECTION_COLUMNS,
            &[
                "course_code",
                "section_code",
                "term",
                "year",
                "capacity",
                "instructor_email",
                "location",
                "schedule_note",
                "notes",
            ]
        );
    }
}
