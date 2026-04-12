//! Typed wrappers for the Phase 5 dashboard panel endpoints.
//!
//! Each panel is fetched via a dedicated GET endpoint under
//! `/dashboards/<key>`. The backend takes an optional RFC3339 `from`
//! and `to` window and an optional `department_id` scope; for
//! non-admin callers the scope is enforced server-side regardless of
//! what the caller passes.

use chrono::{DateTime, SecondsFormat, Utc};
use serde::Deserialize;
use serde_json::Value as JsonValue;

use crate::api::client::{ApiClient, ApiError};

/// A single row inside a [`DashboardPanel`].
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct DashboardRow {
    pub label: String,
    pub value: f64,
    #[serde(default)]
    pub secondary: Option<JsonValue>,
}

/// The full panel envelope returned by every dashboard endpoint.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct DashboardPanel {
    pub metric_key: String,
    #[serde(default)]
    pub window_from: Option<String>,
    #[serde(default)]
    pub window_to: Option<String>,
    #[serde(default)]
    pub department_scope: Option<String>,
    #[serde(default)]
    pub rows: Vec<DashboardRow>,
    #[serde(default)]
    pub notes: Vec<String>,
}

fn client(token: &str) -> ApiClient {
    ApiClient::new(Some(token.to_string()))
}

/// Formats an optional [`DateTime<Utc>`] as an RFC3339 string suitable
/// for passing as a `from` / `to` query parameter.
fn rfc3339(ts: Option<DateTime<Utc>>) -> Option<String> {
    ts.map(|t| t.to_rfc3339_opts(SecondsFormat::Secs, true))
}

/// Builds a query string from the common filter triple.
fn build_query(
    from: Option<DateTime<Utc>>,
    to: Option<DateTime<Utc>>,
    department_id: Option<&str>,
) -> String {
    let mut parts: Vec<String> = Vec::new();
    if let Some(f) = rfc3339(from) {
        parts.push(format!("from={}", urlencode(&f)));
    }
    if let Some(t) = rfc3339(to) {
        parts.push(format!("to={}", urlencode(&t)));
    }
    if let Some(dept) = department_id {
        if !dept.is_empty() {
            parts.push(format!("department_id={}", urlencode(dept)));
        }
    }
    if parts.is_empty() {
        String::new()
    } else {
        format!("?{}", parts.join("&"))
    }
}

/// Minimal percent-encoder that covers the characters that appear in
/// RFC3339 timestamps and UUIDs without pulling in an extra crate.
fn urlencode(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for b in input.bytes() {
        match b {
            b'0'..=b'9' | b'A'..=b'Z' | b'a'..=b'z' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

async fn fetch_panel(
    token: &str,
    key: &str,
    from: Option<DateTime<Utc>>,
    to: Option<DateTime<Utc>>,
    department_id: Option<&str>,
) -> Result<DashboardPanel, ApiError> {
    let path = format!("/dashboards/{}{}", key, build_query(from, to, department_id));
    client(token).get_json(&path).await
}

/// `GET /dashboards/course-popularity`
pub async fn course_popularity(
    token: &str,
    from: Option<DateTime<Utc>>,
    to: Option<DateTime<Utc>>,
    department_id: Option<&str>,
) -> Result<DashboardPanel, ApiError> {
    fetch_panel(token, "course-popularity", from, to, department_id).await
}

/// `GET /dashboards/fill-rate`
pub async fn fill_rate(
    token: &str,
    from: Option<DateTime<Utc>>,
    to: Option<DateTime<Utc>>,
    department_id: Option<&str>,
) -> Result<DashboardPanel, ApiError> {
    fetch_panel(token, "fill-rate", from, to, department_id).await
}

/// `GET /dashboards/drop-rate`
pub async fn drop_rate(
    token: &str,
    from: Option<DateTime<Utc>>,
    to: Option<DateTime<Utc>>,
    department_id: Option<&str>,
) -> Result<DashboardPanel, ApiError> {
    fetch_panel(token, "drop-rate", from, to, department_id).await
}

/// `GET /dashboards/instructor-workload`
pub async fn instructor_workload(
    token: &str,
    from: Option<DateTime<Utc>>,
    to: Option<DateTime<Utc>>,
    department_id: Option<&str>,
) -> Result<DashboardPanel, ApiError> {
    fetch_panel(token, "instructor-workload", from, to, department_id).await
}

/// `GET /dashboards/foot-traffic`
pub async fn foot_traffic(
    token: &str,
    from: Option<DateTime<Utc>>,
    to: Option<DateTime<Utc>>,
    department_id: Option<&str>,
) -> Result<DashboardPanel, ApiError> {
    fetch_panel(token, "foot-traffic", from, to, department_id).await
}

/// `GET /dashboards/dwell-time`
pub async fn dwell_time(
    token: &str,
    from: Option<DateTime<Utc>>,
    to: Option<DateTime<Utc>>,
    department_id: Option<&str>,
) -> Result<DashboardPanel, ApiError> {
    fetch_panel(token, "dwell-time", from, to, department_id).await
}

/// `GET /dashboards/interaction-quality`
pub async fn interaction_quality(
    token: &str,
    from: Option<DateTime<Utc>>,
    to: Option<DateTime<Utc>>,
    department_id: Option<&str>,
) -> Result<DashboardPanel, ApiError> {
    fetch_panel(token, "interaction-quality", from, to, department_id).await
}
