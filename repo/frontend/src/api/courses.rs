//! Typed wrappers for the course endpoints.
//!
//! Courses follow the same author-controlled workflow as journals:
//! every mutation produces a new versioned draft, which can then be
//! approved and published. Prerequisites are managed inline on the
//! course entity. UUIDs are modelled as plain `String`s so the frontend
//! doesn't need to pull in the `uuid` crate.

use serde::{Deserialize, Serialize};

use crate::api::client::{ApiClient, ApiError};

/// A single course version as delivered by the backend.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct CourseVersionView {
    pub id: String,
    pub course_id: String,
    pub version_number: i32,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub syllabus: Option<String>,
    #[serde(default)]
    pub credit_hours: Option<f32>,
    #[serde(default)]
    pub contact_hours: Option<f32>,
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

/// A prerequisite reference as delivered by the backend.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct PrerequisiteRef {
    pub prerequisite_course_id: String,
    pub prerequisite_code: String,
    #[serde(default)]
    pub min_grade: Option<String>,
}

/// A course catalogue entry with its effective version pre-hydrated.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct CourseView {
    pub id: String,
    pub code: String,
    pub title: String,
    #[serde(default)]
    pub department_id: Option<String>,
    #[serde(default)]
    pub owner_id: Option<String>,
    pub is_active: bool,
    #[serde(default)]
    pub current_version_id: Option<String>,
    #[serde(default)]
    pub latest_version_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    #[serde(default)]
    pub effective_version: Option<CourseVersionView>,
    #[serde(default)]
    pub prerequisites: Vec<PrerequisiteRef>,
}

/// Body accepted by `POST /courses`.
#[derive(Debug, Clone, Serialize)]
pub struct CourseCreateInput {
    pub code: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub department_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub syllabus: Option<String>,
    pub credit_hours: f32,
    pub contact_hours: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub change_summary: Option<String>,
}

/// Body accepted by `PUT /courses/<id>`.
#[derive(Debug, Clone, Serialize)]
pub struct CourseEditInput {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub syllabus: Option<String>,
    pub credit_hours: f32,
    pub contact_hours: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub change_summary: Option<String>,
}

/// Body accepted by `POST /courses/<id>/prerequisites`.
#[derive(Debug, Clone, Serialize)]
pub struct AddPrerequisiteInput {
    pub prerequisite_course_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_grade: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct OkResponse {
    #[allow(dead_code)]
    ok: bool,
}

fn client(token: &str) -> ApiClient {
    ApiClient::new(Some(token.to_string()))
}

/// Fetches the paginated course catalogue.
pub async fn list(
    token: &str,
    department_id: Option<&str>,
    limit: u32,
    offset: u32,
) -> Result<Vec<CourseView>, ApiError> {
    let mut query = format!("limit={}&offset={}", limit, offset);
    if let Some(dept) = department_id {
        if !dept.is_empty() {
            query.push_str(&format!("&department_id={}", dept));
        }
    }
    let path = format!("/courses?{}", query);
    client(token).get_json(&path).await
}

/// Fetches a single course by id, including its effective version and
/// prerequisites.
pub async fn get(token: &str, id: &str) -> Result<CourseView, ApiError> {
    let path = format!("/courses/{}", id);
    client(token).get_json(&path).await
}

/// Creates a new course and returns the hydrated view.
pub async fn create(
    token: &str,
    input: &CourseCreateInput,
) -> Result<CourseView, ApiError> {
    client(token).post_json("/courses", input).await
}

/// Edits a course by creating a new draft version.
pub async fn edit_draft(
    token: &str,
    id: &str,
    input: &CourseEditInput,
) -> Result<CourseVersionView, ApiError> {
    let path = format!("/courses/{}", id);
    client(token).put_json(&path, input).await
}

/// Lists every version for a course.
pub async fn list_versions(
    token: &str,
    id: &str,
) -> Result<Vec<CourseVersionView>, ApiError> {
    let path = format!("/courses/{}/versions", id);
    client(token).get_json(&path).await
}

/// Transitions a draft version to `approved`.
pub async fn approve(
    token: &str,
    id: &str,
    version_id: &str,
) -> Result<CourseVersionView, ApiError> {
    let path = format!("/courses/{}/versions/{}/approve", id, version_id);
    client(token).post_no_body_with_result(&path).await
}

/// Publishes an approved version, returning the updated course view.
pub async fn publish(
    token: &str,
    id: &str,
    version_id: &str,
) -> Result<CourseView, ApiError> {
    let path = format!("/courses/{}/versions/{}/publish", id, version_id);
    client(token).post_no_body_with_result(&path).await
}

/// Lists the current prerequisites for a course.
pub async fn list_prerequisites(
    token: &str,
    id: &str,
) -> Result<Vec<PrerequisiteRef>, ApiError> {
    let path = format!("/courses/{}/prerequisites", id);
    client(token).get_json(&path).await
}

/// Adds a prerequisite to a course.
pub async fn add_prerequisite(
    token: &str,
    id: &str,
    input: &AddPrerequisiteInput,
) -> Result<(), ApiError> {
    let path = format!("/courses/{}/prerequisites", id);
    let _: OkResponse = client(token).post_json(&path, input).await?;
    Ok(())
}

/// Removes a prerequisite from a course.
pub async fn remove_prerequisite(
    token: &str,
    id: &str,
    prereq_id: &str,
) -> Result<(), ApiError> {
    let path = format!("/courses/{}/prerequisites/{}", id, prereq_id);
    client(token).delete(&path).await
}
