//! Typed wrappers for the attachment endpoints.
//!
//! Attachments live alongside journals and teaching resources. They
//! carry binary payloads (uploaded via multipart/form-data) and expose
//! a checksum that the backend re-verifies before serving a preview.

use gloo_net::http::Request;
use js_sys::{Array, Uint8Array};
use serde::{Deserialize, Serialize};
use wasm_bindgen::{JsCast, JsValue};
use web_sys::{Blob, FormData, Url};

use crate::api::client::{ApiClient, ApiError, API_BASE};

/// Parent container a given attachment is bound to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ParentType {
    Journal,
    TeachingResource,
}

impl ParentType {
    /// Returns the wire representation for this variant.
    pub fn as_snake(&self) -> &'static str {
        match self {
            ParentType::Journal => "journal",
            ParentType::TeachingResource => "teaching_resource",
        }
    }
}

/// Attachment view as delivered by the backend.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct AttachmentView {
    pub id: String,
    pub parent_type: String,
    pub parent_id: String,
    pub original_filename: String,
    pub mime_type: String,
    pub size_bytes: i64,
    pub sha256_checksum: String,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub uploaded_by: Option<String>,
    pub created_at: String,
    pub is_previewable: bool,
}

fn client(token: &str) -> ApiClient {
    ApiClient::new(Some(token.to_string()))
}

/// Lists attachments for a given parent (journal or teaching resource).
pub async fn list_for_parent(
    token: &str,
    parent_type: ParentType,
    parent_id: &str,
) -> Result<Vec<AttachmentView>, ApiError> {
    let path = format!(
        "/attachments?parent_type={}&parent_id={}",
        parent_type.as_snake(),
        parent_id
    );
    client(token).get_json(&path).await
}

/// Fetches a single attachment's metadata by id.
pub async fn get(token: &str, id: &str) -> Result<AttachmentView, ApiError> {
    let path = format!("/attachments/{}", id);
    client(token).get_json(&path).await
}

/// Permanently deletes an attachment.
pub async fn delete(token: &str, id: &str) -> Result<(), ApiError> {
    let path = format!("/attachments/{}", id);
    client(token).delete(&path).await
}

/// Uploads a file as a new attachment.
///
/// The body is constructed as a browser `FormData` instance so that the
/// browser sets the correct multipart boundary. We attach a `Blob`
/// wrapping the supplied bytes with the provided MIME type.
pub async fn upload(
    token: &str,
    parent_type: ParentType,
    parent_id: &str,
    file_name: &str,
    mime: &str,
    bytes: Vec<u8>,
    category: Option<&str>,
) -> Result<AttachmentView, ApiError> {
    // Build the FormData on the JS side.
    let form = FormData::new()
        .map_err(|e| ApiError::decode(format!("failed to allocate FormData: {:?}", e)))?;

    // Copy the byte buffer into a JS Uint8Array and wrap it in a Blob.
    // Setting the Blob's `type` via a plain JS options object so we are
    // resilient to web-sys BlobPropertyBag API drift.
    let uint8 = Uint8Array::new_with_length(bytes.len() as u32);
    uint8.copy_from(&bytes);
    let parts = Array::new();
    parts.push(&uint8.buffer());
    let options = js_sys::Object::new();
    js_sys::Reflect::set(
        &options,
        &JsValue::from_str("type"),
        &JsValue::from_str(mime),
    )
    .map_err(|e| ApiError::decode(format!("failed to set Blob type: {:?}", e)))?;
    let blob = Blob::new_with_u8_array_sequence_and_options(&parts, options.unchecked_ref())
        .map_err(|e| ApiError::decode(format!("failed to allocate Blob: {:?}", e)))?;

    form.append_with_blob_and_filename("file", &blob, file_name)
        .map_err(|e| ApiError::decode(format!("failed to append file: {:?}", e)))?;
    form.append_with_str("parent_type", parent_type.as_snake())
        .map_err(|e| ApiError::decode(format!("failed to append parent_type: {:?}", e)))?;
    form.append_with_str("parent_id", parent_id)
        .map_err(|e| ApiError::decode(format!("failed to append parent_id: {:?}", e)))?;
    if let Some(cat) = category {
        if !cat.is_empty() {
            form.append_with_str("category", cat)
                .map_err(|e| ApiError::decode(format!("failed to append category: {:?}", e)))?;
        }
    }

    let url = format!("{}/attachments", API_BASE);
    // gloo-net forwards any `JsValue` we give it as the request body;
    // passing a FormData makes the browser set the multipart headers.
    let request = Request::post(&url)
        .header("Authorization", &format!("Bearer {}", token))
        .body(JsValue::from(form))
        .map_err(|e| ApiError::decode(format!("failed to build request: {e}")))?;
    let response = request
        .send()
        .await
        .map_err(|e| ApiError::network(format!("upload failed: {e}")))?;

    if response.ok() {
        response
            .json::<AttachmentView>()
            .await
            .map_err(|e| ApiError::decode(format!("failed to decode response: {e}")))
    } else {
        Err(parse_fetch_error(response).await)
    }
}

/// Builds the relative preview URL for an attachment. The browser can
/// load this directly via `<img src>` / `<iframe src>` **only** when the
/// endpoint is public — because the Scholarly backend requires a bearer
/// token, prefer [`fetch_preview_blob_url`] instead.
pub async fn preview_url(id: &str) -> String {
    format!("{}/attachments/{}/preview", API_BASE, id)
}

/// Fetches the preview bytes for an attachment with the supplied auth
/// token, materialises a `blob:` URL that can be opened in a new tab,
/// and returns the URL alongside the checksum the backend asserted in
/// the `X-Attachment-Checksum` response header.
///
/// The caller is responsible for eventually calling
/// `URL.revokeObjectURL(...)` on the returned URL when it is no longer
/// needed.
pub async fn fetch_preview_blob_url(
    token: &str,
    id: &str,
) -> Result<(String, String), ApiError> {
    let url = format!("{}/attachments/{}/preview", API_BASE, id);
    let response = Request::get(&url)
        .header("Authorization", &format!("Bearer {}", token))
        .send()
        .await
        .map_err(|e| ApiError::network(format!("preview fetch failed: {e}")))?;

    if !response.ok() {
        return Err(parse_fetch_error(response).await);
    }

    let checksum = response
        .headers()
        .get("X-Attachment-Checksum")
        .unwrap_or_else(|| "sha256:unknown".to_string());
    let content_type = response
        .headers()
        .get("Content-Type")
        .unwrap_or_else(|| "application/octet-stream".to_string());

    // Read the response body as raw bytes, then copy them into a JS
    // Uint8Array / Blob so the browser can materialise a `blob:` URL.
    let bytes = response
        .binary()
        .await
        .map_err(|e| ApiError::network(format!("failed to read preview body: {e}")))?;

    let uint8 = Uint8Array::new_with_length(bytes.len() as u32);
    uint8.copy_from(&bytes);
    let parts = Array::new();
    parts.push(&uint8.buffer());
    let options = js_sys::Object::new();
    js_sys::Reflect::set(
        &options,
        &JsValue::from_str("type"),
        &JsValue::from_str(&content_type),
    )
    .map_err(|e| ApiError::decode(format!("failed to set Blob type: {:?}", e)))?;
    let blob = Blob::new_with_u8_array_sequence_and_options(&parts, options.unchecked_ref())
        .map_err(|e| ApiError::decode(format!("failed to allocate Blob: {:?}", e)))?;

    let object_url = Url::create_object_url_with_blob(&blob)
        .map_err(|e| ApiError::decode(format!("failed to create object URL: {:?}", e)))?;

    Ok((object_url, checksum))
}

// ---------------------------------------------------------------------------
// Error decoding — the attachment endpoints follow the same envelope as
// the rest of the API but we can't reuse ApiClient's private
// parse_error helper, so we re-implement the equivalent locally.
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct ErrorEnvelope {
    error: ErrorBody,
}

#[derive(Debug, Deserialize)]
struct ErrorBody {
    code: Option<String>,
    message: Option<String>,
    #[allow(dead_code)]
    request_id: Option<String>,
}

async fn parse_fetch_error(response: gloo_net::http::Response) -> ApiError {
    let status = response.status();
    let fallback_code = match status {
        401 => "unauthorized".to_string(),
        403 => "forbidden".to_string(),
        _ => "http_error".to_string(),
    };
    let fallback_message = format!("HTTP {}", status);
    match response.json::<ErrorEnvelope>().await {
        Ok(env) => {
            let code = if status == 401 {
                "unauthorized".to_string()
            } else {
                env.error.code.unwrap_or_else(|| fallback_code.clone())
            };
            let message = env.error.message.unwrap_or(fallback_message);
            ApiError {
                code,
                message,
                status,
            }
        }
        Err(_) => ApiError {
            code: fallback_code,
            message: fallback_message,
            status,
        },
    }
}
