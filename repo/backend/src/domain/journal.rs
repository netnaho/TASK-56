use serde::{Serialize, Deserialize};
use chrono::NaiveDateTime;
use uuid::Uuid;

/// A reflective journal entry authored by a user (typically a student).
///
/// Journals are versioned: every edit produces a new `JournalVersion` row
/// so that the complete history of changes is preserved.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Journal {
    pub id: Uuid,

    /// The user who created the journal.
    pub author_id: Uuid,

    /// Short title summarising the entry.
    pub title: String,

    /// Whether the journal is visible to instructors / peers.
    /// TODO: refine sharing model (private / instructor-only / public) in phase 2.
    pub is_published: bool,

    /// Pointer to the current (latest) version.
    pub current_version_id: Option<Uuid>,

    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

/// An immutable snapshot of a journal entry at a point in time.
///
/// Each edit creates a new version; the parent `Journal` always points at the
/// latest one via `current_version_id`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JournalVersion {
    pub id: Uuid,

    /// The journal this version belongs to.
    pub journal_id: Uuid,

    /// Monotonically increasing version number (1-based).
    pub version_number: i32,

    /// Full body content of the journal at this version.
    /// TODO: decide on rich-text format (Markdown / HTML / ProseMirror JSON) in phase 2.
    pub body: String,

    /// Optional short note describing what changed in this version.
    pub change_summary: Option<String>,

    /// Who created this version (may differ from original author for collaborative editing).
    pub edited_by: Uuid,

    pub created_at: NaiveDateTime,
}
