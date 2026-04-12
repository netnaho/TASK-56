use serde::{Serialize, Deserialize};
use chrono::NaiveDateTime;
use uuid::Uuid;

/// A course offered within the scholarly platform.
///
/// Courses are top-level containers that hold one or more `Section`s and may
/// reference many `TeachingResource`s.  Courses are versioned so that
/// catalogue changes are tracked over time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Course {
    pub id: Uuid,

    /// The primary instructor or department that owns the course.
    pub owner_id: Uuid,

    /// Catalogue code, e.g. "CS-201".
    pub code: String,

    /// Full course title.
    pub title: String,

    /// Whether the course is visible to students.
    pub is_published: bool,

    /// Pointer to the current (latest) version.
    pub current_version_id: Option<Uuid>,

    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

/// An immutable snapshot of course metadata at a point in time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CourseVersion {
    pub id: Uuid,

    /// The course this version belongs to.
    pub course_id: Uuid,

    /// Monotonically increasing version number (1-based).
    pub version_number: i32,

    /// Long-form course description shown on the catalogue page.
    pub description: Option<String>,

    /// Structured syllabus content.
    /// TODO: decide on format (Markdown / JSON outline) in phase 2.
    pub syllabus: Option<String>,

    /// Expected number of credit hours.
    /// TODO: confirm whether credit hours belong here or on an enrolment entity in phase 2.
    pub credit_hours: Option<i32>,

    /// Who published this version.
    pub published_by: Uuid,

    pub created_at: NaiveDateTime,
}
