use serde::{Serialize, Deserialize};
use chrono::NaiveDateTime;
use uuid::Uuid;

/// A section (class instance) within a course for a specific term.
///
/// While a `Course` represents the abstract catalogue entry, a `Section`
/// represents an actual offering with a schedule, enrolled students, and
/// an assigned instructor.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Section {
    pub id: Uuid,

    /// The course this section belongs to.
    pub course_id: Uuid,

    /// The instructor assigned to teach this section.
    pub instructor_id: Uuid,

    /// Section identifier, e.g. "A", "001".
    pub section_code: String,

    /// Academic term label, e.g. "Fall 2026".
    /// TODO: consider a dedicated `Term` entity in phase 2.
    pub term: String,

    /// Maximum number of students that may enrol.
    pub capacity: Option<i32>,

    /// Pointer to the current (latest) version.
    pub current_version_id: Option<Uuid>,

    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

/// An immutable snapshot of section-level details at a point in time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SectionVersion {
    pub id: Uuid,

    /// The section this version belongs to.
    pub section_id: Uuid,

    /// Monotonically increasing version number (1-based).
    pub version_number: i32,

    /// Room or location where the section meets.
    /// TODO: decide on location modelling (free text vs structured) in phase 2.
    pub location: Option<String>,

    /// Cron-like or human-readable schedule string, e.g. "MWF 10:00–10:50".
    /// TODO: evaluate structured schedule representation in phase 2.
    pub schedule: Option<String>,

    /// Additional notes visible to enrolled students.
    pub notes: Option<String>,

    /// Who published this version.
    pub published_by: Uuid,

    pub created_at: NaiveDateTime,
}
