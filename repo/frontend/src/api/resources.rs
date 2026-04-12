//! Typed wrappers for the teaching-resource endpoints.
//!
//! The shape mirrors [`crate::api::journals`] but substitutes the body
//! text for a richer payload that covers documents, videos,
//! presentations, assessments, external links and datasets.

use serde::{Deserialize, Serialize};

use crate::api::client::{ApiClient, ApiError};

/// Enumeration of resource categories accepted by the backend.
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
    /// Human-friendly label rendered in the UI.
    pub fn label(&self) -> &'static str {
        match self {
            ResourceType::Document => "Document",
            ResourceType::Video => "Video",
            ResourceType::Presentation => "Presentation",
            ResourceType::Assessment => "Assessment",
            ResourceType::ExternalLink => "External Link",
            ResourceType::Dataset => "Dataset",
            ResourceType::Other => "Other",
        }
    }

    /// Wire value — kept in sync with the backend enum.
    pub fn as_snake(&self) -> &'static str {
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

    /// Returns every variant in the canonical UI order. Used by the
    /// create form's resource-type picker.
    pub fn all() -> &'static [ResourceType] {
        &[
            ResourceType::Document,
            ResourceType::Video,
            ResourceType::Presentation,
            ResourceType::Assessment,
            ResourceType::ExternalLink,
            ResourceType::Dataset,
            ResourceType::Other,
        ]
    }

    /// Parses a snake-case string (as emitted by the backend). Returns
    /// `ResourceType::Other` for unknown values so that the UI degrades
    /// gracefully if the backend adds new variants.
    pub fn from_snake(value: &str) -> ResourceType {
        match value {
            "document" => ResourceType::Document,
            "video" => ResourceType::Video,
            "presentation" => ResourceType::Presentation,
            "assessment" => ResourceType::Assessment,
            "external_link" => ResourceType::ExternalLink,
            "dataset" => ResourceType::Dataset,
            _ => ResourceType::Other,
        }
    }
}

/// A single resource version as delivered by the backend.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct ResourceVersionView {
    pub id: String,
    pub resource_id: String,
    pub version_number: i32,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub content_url: Option<String>,
    #[serde(default)]
    pub mime_type: Option<String>,
    #[serde(default)]
    pub size_bytes: Option<i64>,
    #[serde(default)]
    pub change_summary: Option<String>,
    pub state: String,
    #[serde(default)]
    pub created_by: Option<String>,
    pub created_at: String,
    #[serde(default)]
    pub approved_at: Option<String>,
    #[serde(default)]
    pub published_at: Option<String>,
}

/// A teaching-resource catalogue entry with its effective version
/// pre-hydrated.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct ResourceView {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub abstract_text: Option<String>,
    #[serde(default)]
    pub author_id: Option<String>,
    pub is_published: bool,
    #[serde(default)]
    pub current_version_id: Option<String>,
    #[serde(default)]
    pub latest_version_id: Option<String>,
    #[serde(default)]
    pub resource_type: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    #[serde(default)]
    pub effective_version: Option<ResourceVersionView>,
}

/// Body accepted by `POST /teaching-resources`.
#[derive(Debug, Clone, Serialize)]
pub struct ResourceCreateInput {
    pub title: String,
    pub resource_type: ResourceType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub change_summary: Option<String>,
}

/// Body accepted by `PUT /teaching-resources/<id>`.
#[derive(Debug, Clone, Serialize)]
pub struct ResourceEditInput {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub change_summary: Option<String>,
}

fn client(token: &str) -> ApiClient {
    ApiClient::new(Some(token.to_string()))
}

/// Fetches the paginated teaching-resource catalogue.
pub async fn list(token: &str, limit: u32, offset: u32) -> Result<Vec<ResourceView>, ApiError> {
    let path = format!("/teaching-resources?limit={}&offset={}", limit, offset);
    client(token).get_json(&path).await
}

/// Fetches a single resource by id, including its effective version.
pub async fn get(token: &str, id: &str) -> Result<ResourceView, ApiError> {
    let path = format!("/teaching-resources/{}", id);
    client(token).get_json(&path).await
}

/// Creates a new teaching resource and returns the hydrated view.
pub async fn create(
    token: &str,
    input: &ResourceCreateInput,
) -> Result<ResourceView, ApiError> {
    client(token).post_json("/teaching-resources", input).await
}

/// Edits a resource by creating a new draft version.
pub async fn edit(
    token: &str,
    id: &str,
    input: &ResourceEditInput,
) -> Result<ResourceVersionView, ApiError> {
    let path = format!("/teaching-resources/{}", id);
    client(token).put_json(&path, input).await
}

/// Lists every version for a resource.
pub async fn list_versions(
    token: &str,
    id: &str,
) -> Result<Vec<ResourceVersionView>, ApiError> {
    let path = format!("/teaching-resources/{}/versions", id);
    client(token).get_json(&path).await
}

/// Fetches a specific resource version by id.
pub async fn get_version(
    token: &str,
    id: &str,
    version_id: &str,
) -> Result<ResourceVersionView, ApiError> {
    let path = format!("/teaching-resources/{}/versions/{}", id, version_id);
    client(token).get_json(&path).await
}

/// Transitions a draft version to `approved`.
pub async fn approve(
    token: &str,
    id: &str,
    version_id: &str,
) -> Result<ResourceVersionView, ApiError> {
    let path = format!("/teaching-resources/{}/versions/{}/approve", id, version_id);
    client(token).post_no_body_with_result(&path).await
}

/// Publishes an approved version, returning the updated resource view.
pub async fn publish(
    token: &str,
    id: &str,
    version_id: &str,
) -> Result<ResourceView, ApiError> {
    let path = format!("/teaching-resources/{}/versions/{}/publish", id, version_id);
    client(token).post_no_body_with_result(&path).await
}
