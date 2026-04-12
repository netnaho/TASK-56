//! Typed wrappers for the journal endpoints.
//!
//! The backend exposes an author-controlled workflow: every mutation
//! produces a new versioned draft, which can later be approved and
//! published. These helpers mirror that flow one-to-one so that pages
//! never have to hand-roll HTTP calls.

use serde::{Deserialize, Serialize};

use crate::api::client::{ApiClient, ApiError};

/// A single journal version as delivered by the backend.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct JournalVersionView {
    pub id: String,
    pub journal_id: String,
    pub version_number: i32,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub body: Option<String>,
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

/// A journal catalogue entry with its effective version pre-hydrated.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct JournalView {
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
    pub created_at: String,
    pub updated_at: String,
    #[serde(default)]
    pub effective_version: Option<JournalVersionView>,
}

/// Body accepted by `POST /journals`.
#[derive(Debug, Clone, Serialize)]
pub struct JournalCreateInput {
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub abstract_text: Option<String>,
    pub body: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub change_summary: Option<String>,
}

/// Body accepted by `PUT /journals/<id>`.
#[derive(Debug, Clone, Serialize)]
pub struct JournalEditInput {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub body: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub change_summary: Option<String>,
}

fn client(token: &str) -> ApiClient {
    ApiClient::new(Some(token.to_string()))
}

/// Fetches the paginated journal catalogue.
pub async fn list(token: &str, limit: u32, offset: u32) -> Result<Vec<JournalView>, ApiError> {
    let path = format!("/journals?limit={}&offset={}", limit, offset);
    client(token).get_json(&path).await
}

/// Fetches a single journal by id, including its effective version.
pub async fn get(token: &str, id: &str) -> Result<JournalView, ApiError> {
    let path = format!("/journals/{}", id);
    client(token).get_json(&path).await
}

/// Creates a new journal and returns the hydrated view.
pub async fn create(
    token: &str,
    input: &JournalCreateInput,
) -> Result<JournalView, ApiError> {
    client(token).post_json("/journals", input).await
}

/// Edits a journal by creating a new draft version.
pub async fn edit(
    token: &str,
    id: &str,
    input: &JournalEditInput,
) -> Result<JournalVersionView, ApiError> {
    let path = format!("/journals/{}", id);
    client(token).put_json(&path, input).await
}

/// Lists every version for a journal (most recent first).
pub async fn list_versions(
    token: &str,
    id: &str,
) -> Result<Vec<JournalVersionView>, ApiError> {
    let path = format!("/journals/{}/versions", id);
    client(token).get_json(&path).await
}

/// Fetches a specific version by id.
pub async fn get_version(
    token: &str,
    id: &str,
    version_id: &str,
) -> Result<JournalVersionView, ApiError> {
    let path = format!("/journals/{}/versions/{}", id, version_id);
    client(token).get_json(&path).await
}

/// Transitions a draft version to `approved`.
pub async fn approve(
    token: &str,
    id: &str,
    version_id: &str,
) -> Result<JournalVersionView, ApiError> {
    let path = format!("/journals/{}/versions/{}/approve", id, version_id);
    client(token).post_no_body_with_result(&path).await
}

/// Publishes an approved version, returning the updated journal view.
pub async fn publish(
    token: &str,
    id: &str,
    version_id: &str,
) -> Result<JournalView, ApiError> {
    let path = format!("/journals/{}/versions/{}/publish", id, version_id);
    client(token).post_no_body_with_result(&path).await
}

