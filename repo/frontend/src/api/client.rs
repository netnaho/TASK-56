//! HTTP client for the Scholarly backend API.
//!
//! The frontend is served from the same origin as the backend thanks to
//! an nginx reverse proxy that forwards `/api/*` to port `8000`, so
//! requests are issued against relative URLs — no CORS concerns.

use gloo_net::http::Request;
use serde::{de::DeserializeOwned, Deserialize, Serialize};

/// Base path for all API calls. Same-origin; nginx proxies upstream.
pub const API_BASE: &str = "/api/v1";

/// Error type returned by every [`ApiClient`] method.
///
/// `code` follows the backend error envelope (e.g. `"unauthorized"`,
/// `"validation"`). `status` is the raw HTTP status code, or `0` when
/// the failure happened before we ever saw a response (network /
/// serialization errors).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApiError {
    pub code: String,
    pub message: String,
    pub status: u16,
}

impl ApiError {
    pub fn network(message: impl Into<String>) -> Self {
        Self {
            code: "network_error".to_string(),
            message: message.into(),
            status: 0,
        }
    }

    pub fn decode(message: impl Into<String>) -> Self {
        Self {
            code: "decode_error".to_string(),
            message: message.into(),
            status: 0,
        }
    }

    /// Returns `true` if this error indicates the session is no longer
    /// valid (HTTP 401 / `unauthorized`).
    pub fn is_unauthorized(&self) -> bool {
        self.status == 401 || self.code == "unauthorized"
    }

    /// Returns `true` if this error indicates the current user is
    /// forbidden from performing the requested action (HTTP 403).
    pub fn is_forbidden(&self) -> bool {
        self.status == 403 || self.code == "forbidden"
    }
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ({}): {}", self.code, self.status, self.message)
    }
}

/// Envelope used by the backend for every error response.
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

/// Lightweight wrapper around `gloo-net` providing convenience methods
/// for authenticated requests to the Scholarly backend.
#[derive(Clone, Debug, Default)]
pub struct ApiClient {
    /// Optional bearer token attached to every outbound request.
    pub token: Option<String>,
}

impl ApiClient {
    /// Creates a new [`ApiClient`] with the provided token (if any).
    pub fn new(token: Option<String>) -> Self {
        Self { token }
    }

    /// Builds an absolute URL for the given API-relative path.
    fn url(&self, path: &str) -> String {
        if path.starts_with('/') {
            format!("{}{}", API_BASE, path)
        } else {
            format!("{}/{}", API_BASE, path)
        }
    }

    /// Attaches the `Authorization` header when a token is configured.
    fn with_auth(&self, mut req: gloo_net::http::RequestBuilder) -> gloo_net::http::RequestBuilder {
        if let Some(token) = &self.token {
            req = req.header("Authorization", &format!("Bearer {}", token));
        }
        req
    }

    /// Issues a `POST` request with a JSON body and decodes a JSON
    /// response.
    pub async fn post_json<I, O>(&self, path: &str, body: &I) -> Result<O, ApiError>
    where
        I: Serialize + ?Sized,
        O: DeserializeOwned,
    {
        let url = self.url(path);
        let builder = self.with_auth(Request::post(&url));
        let request = builder
            .json(body)
            .map_err(|e| ApiError::decode(format!("failed to serialize body: {e}")))?;
        let response = request
            .send()
            .await
            .map_err(|e| ApiError::network(format!("request failed: {e}")))?;

        handle_response::<O>(response).await
    }

    /// Issues a `PUT` request with a JSON body and decodes a JSON
    /// response.
    pub async fn put_json<I, O>(&self, path: &str, body: &I) -> Result<O, ApiError>
    where
        I: Serialize + ?Sized,
        O: DeserializeOwned,
    {
        let url = self.url(path);
        let builder = self.with_auth(Request::put(&url));
        let request = builder
            .json(body)
            .map_err(|e| ApiError::decode(format!("failed to serialize body: {e}")))?;
        let response = request
            .send()
            .await
            .map_err(|e| ApiError::network(format!("request failed: {e}")))?;

        handle_response::<O>(response).await
    }

    /// Issues a `GET` request and decodes a JSON response.
    pub async fn get_json<O>(&self, path: &str) -> Result<O, ApiError>
    where
        O: DeserializeOwned,
    {
        let url = self.url(path);
        let request = self.with_auth(Request::get(&url));
        let response = request
            .send()
            .await
            .map_err(|e| ApiError::network(format!("request failed: {e}")))?;

        handle_response::<O>(response).await
    }

    /// Issues a `POST` request with no body and ignores the response
    /// body on success (used for idempotent no-content endpoints such
    /// as `/auth/logout`).
    pub async fn post_no_body(&self, path: &str) -> Result<(), ApiError> {
        let url = self.url(path);
        let request = self.with_auth(Request::post(&url));
        let response = request
            .send()
            .await
            .map_err(|e| ApiError::network(format!("request failed: {e}")))?;

        if response.ok() {
            return Ok(());
        }
        Err(parse_error(response).await)
    }

    /// Issues a `POST` request with an empty JSON body and decodes the
    /// response as JSON. Used for action endpoints (e.g. approve,
    /// publish) where the backend ignores the request body but returns a
    /// hydrated view on success.
    pub async fn post_no_body_with_result<O>(&self, path: &str) -> Result<O, ApiError>
    where
        O: DeserializeOwned,
    {
        let url = self.url(path);
        let builder = self.with_auth(Request::post(&url));
        let request = builder
            .json(&serde_json::json!({}))
            .map_err(|e| ApiError::decode(format!("failed to serialize body: {e}")))?;
        let response = request
            .send()
            .await
            .map_err(|e| ApiError::network(format!("request failed: {e}")))?;

        handle_response::<O>(response).await
    }

    /// Issues a `DELETE` request and ignores the response body on
    /// success.
    pub async fn delete(&self, path: &str) -> Result<(), ApiError> {
        let url = self.url(path);
        let request = self.with_auth(Request::delete(&url));
        let response = request
            .send()
            .await
            .map_err(|e| ApiError::network(format!("request failed: {e}")))?;

        if response.ok() {
            return Ok(());
        }
        Err(parse_error(response).await)
    }
}

/// Decodes a successful JSON response, or converts a non-2xx response
/// into an [`ApiError`] by parsing the error envelope.
async fn handle_response<O>(response: gloo_net::http::Response) -> Result<O, ApiError>
where
    O: DeserializeOwned,
{
    if response.ok() {
        response
            .json::<O>()
            .await
            .map_err(|e| ApiError::decode(format!("failed to decode response: {e}")))
    } else {
        Err(parse_error(response).await)
    }
}

/// Extracts an [`ApiError`] from a failed response by parsing the
/// backend's error envelope. Falls back to a generic error if the body
/// is missing or malformed. Preserves the literal `"unauthorized"` code
/// for 401 responses as a stable contract for upstream callers.
async fn parse_error(response: gloo_net::http::Response) -> ApiError {
    let status = response.status();
    let fallback_code = match status {
        401 => "unauthorized".to_string(),
        403 => "forbidden".to_string(),
        _ => "http_error".to_string(),
    };
    let fallback_message = format!("HTTP {}", status);

    match response.json::<ErrorEnvelope>().await {
        Ok(envelope) => {
            let code = envelope.error.code.unwrap_or_else(|| fallback_code.clone());
            let message = envelope
                .error
                .message
                .unwrap_or_else(|| fallback_message.clone());
            // For 401 keep the canonical "unauthorized" code regardless
            // of whatever the server sent, as per contract.
            let code = if status == 401 {
                "unauthorized".to_string()
            } else {
                code
            };
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
