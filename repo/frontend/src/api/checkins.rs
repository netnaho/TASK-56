//! Typed wrappers for the Phase 5 check-in endpoints.
//!
//! The backend implements a one-tap check-in flow that returns a rich
//! [`CheckinResult`] envelope describing the outcome (`success`,
//! `duplicate`, `retried`, or `network_blocked`). Duplicate and
//! network-block states are also expressed through HTTP error codes
//! (409 / 403) so the UI can react via [`ApiError::code`]:
//!   - 409 with `code == "conflict"` → trigger the retry flow.
//!   - 403 with `code == "forbidden"` and a network-related message →
//!     show the blocked-by-network-rule banner.

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

use crate::api::client::{ApiClient, ApiError};

/// How the user performed the check-in. Serialised as snake-case for
/// wire compatibility with the backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckinType {
    QrCode,
    Geofence,
    ManualInstructor,
    NfcBeacon,
}

impl CheckinType {
    pub fn as_label(&self) -> &'static str {
        match self {
            CheckinType::QrCode => "QR code",
            CheckinType::Geofence => "Geofence",
            CheckinType::ManualInstructor => "Manual (instructor)",
            CheckinType::NfcBeacon => "NFC beacon",
        }
    }
}

/// Body accepted by `POST /checkins`.
#[derive(Debug, Clone, Serialize)]
pub struct CheckinInput {
    pub section_id: String,
    pub checkin_type: CheckinType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_fingerprint: Option<JsonValue>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub network_hint: Option<String>,
}

/// Body accepted by `POST /checkins/<original_id>/retry`.
#[derive(Debug, Clone, Serialize)]
pub struct CheckinRetryInput {
    pub reason_code: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_fingerprint: Option<JsonValue>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub network_hint: Option<String>,
}

/// A masked check-in record as returned by the backend. For non-admin
/// callers several fields are blanked out server-side; the frontend
/// should render whatever it gets without additional filtering.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct CheckinView {
    pub id: String,
    #[serde(default)]
    pub user_id: Option<String>,
    pub user_display: String,
    #[serde(default)]
    pub user_email: Option<String>,
    pub section_id: String,
    pub checkin_type: String,
    pub checked_in_at: String,
    pub retry_sequence: i32,
    #[serde(default)]
    pub retry_of_id: Option<String>,
    #[serde(default)]
    pub retry_reason: Option<String>,
    pub is_duplicate_attempt: bool,
    pub network_verified: bool,
    #[serde(default)]
    pub network_hint: Option<String>,
    #[serde(default)]
    pub client_ip: Option<String>,
    #[serde(default)]
    pub device_fingerprint: Option<JsonValue>,
}

/// Envelope returned by `POST /checkins` and `POST /checkins/.../retry`
/// on success.
#[derive(Debug, Clone, Deserialize)]
pub struct CheckinResult {
    pub status: String,
    pub view: CheckinView,
    #[serde(default)]
    pub duplicate_window_minutes: Option<i32>,
    #[serde(default)]
    pub network_rule_active: bool,
}

/// One entry in the retry-reason picklist.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct RetryReason {
    pub reason_code: String,
    pub display_name: String,
    #[serde(default)]
    pub description: Option<String>,
}

fn client(token: &str) -> ApiClient {
    ApiClient::new(Some(token.to_string()))
}

/// Performs a check-in. On duplicate the backend responds with HTTP 409
/// (`code = "conflict"`); the caller should inspect the returned
/// [`ApiError`] and branch into the retry flow. Network-rule violations
/// surface as HTTP 403 (`code = "forbidden"`) with a message mentioning
/// the word "network".
pub async fn check_in(
    token: &str,
    input: &CheckinInput,
) -> Result<CheckinResult, ApiError> {
    client(token).post_json("/checkins", input).await
}

/// Retries a previous (non-duplicate) check-in. The backend enforces a
/// hard cap of one retry per original.
pub async fn retry_checkin(
    token: &str,
    original_id: &str,
    input: &CheckinRetryInput,
) -> Result<CheckinResult, ApiError> {
    let path = format!("/checkins/{}/retry", original_id);
    client(token).post_json(&path, input).await
}

/// Lists the most recent check-ins for a section. Server-side masking
/// is already applied for non-admin callers.
pub async fn list_checkins(
    token: &str,
    section_id: &str,
) -> Result<Vec<CheckinView>, ApiError> {
    let path = format!("/checkins?section_id={}", section_id);
    client(token).get_json(&path).await
}

/// Fetches the dictionary of allowed retry reasons.
pub async fn list_retry_reasons(token: &str) -> Result<Vec<RetryReason>, ApiError> {
    client(token).get_json("/checkins/retry-reasons").await
}
