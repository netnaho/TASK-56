//! Typed wrappers for the bulk import/export endpoints.
//!
//! Bulk operations come in three flavours:
//!
//! 1. **Template downloads** — pre-generated CSV/XLSX files the operator
//!    populates with rows to import.
//! 2. **Export downloads** — a dump of the current catalogue in CSV or
//!    XLSX form.
//! 3. **Uploads** — a two-phase workflow with an optional `dry_run`
//!    mode that validates rows without committing, then a `commit`
//!    mode that performs the actual insertion.
//!
//! Both downloads and uploads require a bearer token, so this module
//! exposes [`download_authenticated`] which fetches the bytes, wraps
//! them in a `blob:` URL and triggers a hidden `<a download>` click.

use gloo_net::http::Request;
use js_sys::{Array, Uint8Array};
use serde::Deserialize;
use wasm_bindgen::{JsCast, JsValue};
use web_sys::{Blob, FormData, HtmlAnchorElement, Url};

use crate::api::client::{ApiError, API_BASE};

// ---------------------------------------------------------------------------
// Import report shapes — mirror the backend's response for
// /courses/import and /sections/import.
// ---------------------------------------------------------------------------

/// A single row-level field validation error.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct FieldError {
    pub field: String,
    pub message: String,
}

/// The per-row report inside an [`ImportReport`].
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct RowReport {
    pub row_index: u32,
    pub ok: bool,
    #[serde(default)]
    pub errors: Vec<FieldError>,
    #[serde(default)]
    pub parsed: Option<serde_json::Value>,
}

/// The full response payload returned by a bulk-import call.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct ImportReport {
    pub job_id: String,
    /// `"courses"` or `"sections"`.
    pub kind: String,
    /// `"dry_run"` or `"commit"`.
    pub mode: String,
    /// `"csv"` or `"xlsx"`.
    pub format: String,
    pub total_rows: u32,
    pub valid_rows: u32,
    pub error_rows: u32,
    pub committed: bool,
    #[serde(default)]
    pub rows: Vec<RowReport>,
}

// ---------------------------------------------------------------------------
// URL helpers — the template/export endpoints are consumed via the
// authenticated downloader below, but returning URLs keeps the page
// markup declarative.
// ---------------------------------------------------------------------------

pub fn courses_template_csv_url() -> String {
    "/courses/template.csv".to_string()
}

pub fn courses_template_xlsx_url() -> String {
    "/courses/template.xlsx".to_string()
}

pub fn courses_export_csv_url() -> String {
    "/courses/export.csv".to_string()
}

pub fn courses_export_xlsx_url() -> String {
    "/courses/export.xlsx".to_string()
}

pub fn sections_template_csv_url() -> String {
    "/sections/template.csv".to_string()
}

pub fn sections_template_xlsx_url() -> String {
    "/sections/template.xlsx".to_string()
}

pub fn sections_export_csv_url() -> String {
    "/sections/export.csv".to_string()
}

pub fn sections_export_xlsx_url() -> String {
    "/sections/export.xlsx".to_string()
}

// ---------------------------------------------------------------------------
// Uploads
// ---------------------------------------------------------------------------

async fn upload_import(
    endpoint: &str,
    token: &str,
    file_name: &str,
    mime: &str,
    bytes: Vec<u8>,
    mode: &str,
) -> Result<ImportReport, ApiError> {
    let form = FormData::new()
        .map_err(|e| ApiError::decode(format!("failed to allocate FormData: {:?}", e)))?;

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
    form.append_with_str("mode", mode)
        .map_err(|e| ApiError::decode(format!("failed to append mode: {:?}", e)))?;

    // Mode is passed as both a form field and a query parameter so the
    // backend can dispatch based on either.
    let url = format!("{}{}?mode={}", API_BASE, endpoint, mode);
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
            .json::<ImportReport>()
            .await
            .map_err(|e| ApiError::decode(format!("failed to decode response: {e}")))
    } else {
        Err(parse_fetch_error(response).await)
    }
}

/// Uploads a file to `POST /courses/import`.
pub async fn upload_courses_import(
    token: &str,
    file_name: &str,
    mime: &str,
    bytes: Vec<u8>,
    mode: &str,
) -> Result<ImportReport, ApiError> {
    upload_import("/courses/import", token, file_name, mime, bytes, mode).await
}

/// Uploads a file to `POST /sections/import`.
pub async fn upload_sections_import(
    token: &str,
    file_name: &str,
    mime: &str,
    bytes: Vec<u8>,
    mode: &str,
) -> Result<ImportReport, ApiError> {
    upload_import("/sections/import", token, file_name, mime, bytes, mode).await
}

// ---------------------------------------------------------------------------
// Authenticated downloads
// ---------------------------------------------------------------------------

/// Fetches `path` (relative to `/api/v1`) with a Bearer token, wraps the
/// response bytes in a `blob:` URL and triggers a hidden `<a download>`
/// click so the browser prompts the user to save the file. Revokes the
/// blob URL after kicking off the download.
pub async fn download_authenticated(
    token: &str,
    path: &str,
    filename: &str,
) -> Result<(), ApiError> {
    let url = format!("{}{}", API_BASE, path);
    let response = Request::get(&url)
        .header("Authorization", &format!("Bearer {}", token))
        .send()
        .await
        .map_err(|e| ApiError::network(format!("download failed: {e}")))?;

    if !response.ok() {
        return Err(parse_fetch_error(response).await);
    }

    let content_type = response
        .headers()
        .get("Content-Type")
        .unwrap_or_else(|| "application/octet-stream".to_string());

    let bytes = response
        .binary()
        .await
        .map_err(|e| ApiError::network(format!("failed to read response body: {e}")))?;

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

    // Synthesise a hidden anchor and click it.
    let window = web_sys::window()
        .ok_or_else(|| ApiError::decode("no window available".to_string()))?;
    let document = window
        .document()
        .ok_or_else(|| ApiError::decode("no document available".to_string()))?;
    let anchor_el = document
        .create_element("a")
        .map_err(|e| ApiError::decode(format!("failed to create anchor: {:?}", e)))?;
    let anchor: HtmlAnchorElement = anchor_el
        .dyn_into()
        .map_err(|_| ApiError::decode("anchor cast failed".to_string()))?;
    anchor.set_href(&object_url);
    anchor.set_download(filename);
    // Attach the anchor to the document body so `.click()` reliably
    // triggers the browser's download handling in every engine.
    if let Some(body) = document.body() {
        let _ = body.append_child(&anchor);
        anchor.click();
        let _ = body.remove_child(&anchor);
    } else {
        anchor.click();
    }
    // Best-effort revoke — the browser has already begun the download.
    let _ = Url::revoke_object_url(&object_url);
    Ok(())
}

// ---------------------------------------------------------------------------
// Error decoding — same envelope as the attachment module.
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
