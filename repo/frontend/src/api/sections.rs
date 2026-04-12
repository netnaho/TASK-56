//! Typed wrappers for the section endpoints.
//!
//! Sections belong to a parent course and follow the same versioned
//! draft → approved → published workflow as courses and journals.

use serde::{Deserialize, Serialize};

use crate::api::client::{ApiClient, ApiError};

/// A single section version as delivered by the backend.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct SectionVersionView {
    pub id: String,
    pub section_id: String,
    pub version_number: i32,
    #[serde(default)]
    pub location: Option<String>,
    #[serde(default)]
    pub schedule_note: Option<String>,
    #[serde(default)]
    pub notes: Option<String>,
    pub state: String,
    #[serde(default)]
    pub created_by: Option<String>,
    pub created_at: String,
    #[serde(default)]
    pub approved_at: Option<String>,
    #[serde(default)]
    pub published_at: Option<String>,
}

/// A section catalogue entry with its effective version pre-hydrated.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct SectionView {
    pub id: String,
    pub course_id: String,
    pub course_code: String,
    #[serde(default)]
    pub department_id: Option<String>,
    pub section_code: String,
    pub term: String,
    pub year: i32,
    #[serde(default)]
    pub capacity: Option<i32>,
    #[serde(default)]
    pub instructor_id: Option<String>,
    pub is_active: bool,
    #[serde(default)]
    pub current_version_id: Option<String>,
    #[serde(default)]
    pub latest_version_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    #[serde(default)]
    pub effective_version: Option<SectionVersionView>,
}

/// Body accepted by `POST /sections`.
#[derive(Debug, Clone, Serialize)]
pub struct SectionCreateInput {
    pub course_id: String,
    pub section_code: String,
    pub term: String,
    pub year: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capacity: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instructor_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schedule_note: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub change_summary: Option<String>,
}

/// Body accepted by `PUT /sections/<id>`.
#[derive(Debug, Clone, Serialize)]
pub struct SectionEditInput {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schedule_note: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub change_summary: Option<String>,
}

fn client(token: &str) -> ApiClient {
    ApiClient::new(Some(token.to_string()))
}

/// Fetches the paginated section catalogue, optionally filtered by
/// course.
pub async fn list(
    token: &str,
    course_id: Option<&str>,
    limit: u32,
    offset: u32,
) -> Result<Vec<SectionView>, ApiError> {
    let mut query = format!("limit={}&offset={}", limit, offset);
    if let Some(cid) = course_id {
        if !cid.is_empty() {
            query.push_str(&format!("&course_id={}", cid));
        }
    }
    let path = format!("/sections?{}", query);
    client(token).get_json(&path).await
}

/// Fetches a single section by id, including its effective version.
pub async fn get(token: &str, id: &str) -> Result<SectionView, ApiError> {
    let path = format!("/sections/{}", id);
    client(token).get_json(&path).await
}

/// Creates a new section and returns the hydrated view.
pub async fn create(
    token: &str,
    input: &SectionCreateInput,
) -> Result<SectionView, ApiError> {
    client(token).post_json("/sections", input).await
}

/// Edits a section by creating a new draft version.
pub async fn edit_draft(
    token: &str,
    id: &str,
    input: &SectionEditInput,
) -> Result<SectionVersionView, ApiError> {
    let path = format!("/sections/{}", id);
    client(token).put_json(&path, input).await
}

/// Lists every version for a section.
pub async fn list_versions(
    token: &str,
    id: &str,
) -> Result<Vec<SectionVersionView>, ApiError> {
    let path = format!("/sections/{}/versions", id);
    client(token).get_json(&path).await
}

/// Transitions a draft version to `approved`.
pub async fn approve(
    token: &str,
    id: &str,
    version_id: &str,
) -> Result<SectionVersionView, ApiError> {
    let path = format!("/sections/{}/versions/{}/approve", id, version_id);
    client(token).post_no_body_with_result(&path).await
}

/// Publishes an approved version, returning the updated section view.
pub async fn publish(
    token: &str,
    id: &str,
    version_id: &str,
) -> Result<SectionView, ApiError> {
    let path = format!("/sections/{}/versions/{}/publish", id, version_id);
    client(token).post_no_body_with_result(&path).await
}
