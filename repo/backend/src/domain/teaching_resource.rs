use serde::{Serialize, Deserialize};
use chrono::NaiveDateTime;
use uuid::Uuid;

/// The kind of content a teaching resource represents.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ResourceType {
    /// A written document (PDF, Word, etc.).
    Document,
    /// A video file or external video link.
    Video,
    /// A slide deck / presentation.
    Presentation,
    /// An interactive quiz or assessment.
    Assessment,
    /// An external URL (website, article).
    ExternalLink,
    /// A downloadable dataset or code sample.
    Dataset,
    /// Catch-all for types not yet categorised.
    /// TODO: evaluate whether this should be removed once all types are known in phase 2.
    Other,
}

/// A teaching resource that can be attached to courses and sections.
///
/// Resources are versioned so instructors can update materials without
/// losing the previous revision history.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeachingResource {
    pub id: Uuid,

    /// The instructor or admin who owns this resource.
    pub owner_id: Uuid,

    /// Human-readable title.
    pub title: String,

    /// Classification of the resource.
    pub resource_type: ResourceType,

    /// Whether the resource is publicly discoverable.
    pub is_published: bool,

    /// Pointer to the current (latest) version.
    pub current_version_id: Option<Uuid>,

    /// Optional tags for search and filtering.
    /// TODO: decide on tag storage strategy (array column vs join table) in phase 2.
    pub tags: Vec<String>,

    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

/// An immutable snapshot of a teaching resource's content at a point in time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceVersion {
    pub id: Uuid,

    /// The resource this version belongs to.
    pub resource_id: Uuid,

    /// Monotonically increasing version number (1-based).
    pub version_number: i32,

    /// Description or abstract of the resource content.
    pub description: Option<String>,

    /// Storage key or URL pointing to the actual file/content.
    /// TODO: finalise object-store path convention in phase 2.
    pub content_url: String,

    /// MIME type of the stored content.
    pub mime_type: Option<String>,

    /// Size of the resource in bytes, if applicable.
    /// TODO: confirm whether we store this or compute on read in phase 2.
    pub size_bytes: Option<i64>,

    /// Who created this version.
    pub uploaded_by: Uuid,

    pub created_at: NaiveDateTime,
}
