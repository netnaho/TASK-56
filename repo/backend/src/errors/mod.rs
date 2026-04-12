//! Unified error type and API error envelope.
//!
//! # Error envelope
//!
//! Every API failure produces a JSON body matching:
//!
//! ```json
//! {
//!   "error": {
//!     "code":    "machine_readable_code",
//!     "message": "Human readable message",
//!     "fields":  { "password": ["too_short"] },
//!     "request_id": "uuid-v4"
//!   }
//! }
//! ```
//!
//! `fields` is present only for validation errors. `request_id` is attached
//! so server-side log entries can be correlated with client reports.

use std::collections::HashMap;
use std::io::Cursor;

use rocket::http::{ContentType, Status};
use rocket::response::{self, Responder, Response};
use rocket::Request;
use serde::Serialize;
use thiserror::Error;
use uuid::Uuid;

/// Unified application error type used throughout the backend.
#[derive(Debug, Error)]
pub enum AppError {
    /// No (or invalid) credentials were supplied.
    #[error("Authentication required")]
    Unauthorized,

    /// Credentials are valid but the principal lacks permission.
    #[error("Insufficient permissions")]
    Forbidden,

    /// Generic "not found" with a caller-provided label.
    #[error("Resource not found: {0}")]
    NotFound(String),

    /// Input failed validation.
    #[error("Validation error: {0}")]
    Validation(String),

    /// Input failed validation with per-field detail.
    #[error("Validation error")]
    FieldValidation(HashMap<String, Vec<String>>),

    /// A uniqueness or workflow conflict.
    #[error("Conflict: {0}")]
    Conflict(String),

    /// Account has been locked due to too many failed logins.
    #[error("Account locked: {0}")]
    AccountLocked(String),

    /// Retention execution was blocked because actionable legacy artifacts
    /// (artifact_dek IS NULL, not permanently terminal) still exist under
    /// the retention cutoff.  Callers should run the backfill endpoint first.
    #[error("strict_mode_blocked: {unresolved_count} unresolved legacy artifact(s) \
             require backfill before retention can proceed safely. {hint}")]
    StrictModeBlocked {
        /// Count of expired artifact rows that are actionable legacy.
        unresolved_count: u64,
        /// Human-readable remediation hint.
        hint: String,
    },

    /// Unexpected server-side failure.
    #[error("Internal server error: {0}")]
    Internal(String),

    /// Failure originating from the database layer.
    #[error("Database error: {0}")]
    Database(String),
}

impl From<sqlx::Error> for AppError {
    fn from(err: sqlx::Error) -> Self {
        match err {
            sqlx::Error::RowNotFound => AppError::NotFound("row".into()),
            other => AppError::Database(other.to_string()),
        }
    }
}

/// Machine-readable error code shown to API clients.
///
/// Clients should branch on this instead of parsing the human message.
#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    Unauthorized,
    Forbidden,
    NotFound,
    Validation,
    Conflict,
    AccountLocked,
    StrictModeBlocked,
    Internal,
    Database,
}

#[derive(Debug, Serialize)]
pub struct ErrorBody {
    pub code: ErrorCode,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fields: Option<HashMap<String, Vec<String>>>,
    pub request_id: String,
}

#[derive(Debug, Serialize)]
pub struct ErrorEnvelope {
    pub error: ErrorBody,
}

impl AppError {
    pub fn status(&self) -> Status {
        match self {
            AppError::Unauthorized => Status::Unauthorized,
            AppError::Forbidden => Status::Forbidden,
            AppError::NotFound(_) => Status::NotFound,
            AppError::Validation(_) | AppError::FieldValidation(_) => Status::UnprocessableEntity,
            AppError::Conflict(_) => Status::Conflict,
            AppError::AccountLocked(_) => Status::TooManyRequests,
            AppError::StrictModeBlocked { .. } => Status::Conflict,
            AppError::Internal(_) | AppError::Database(_) => Status::InternalServerError,
        }
    }

    pub fn code(&self) -> ErrorCode {
        match self {
            AppError::Unauthorized => ErrorCode::Unauthorized,
            AppError::Forbidden => ErrorCode::Forbidden,
            AppError::NotFound(_) => ErrorCode::NotFound,
            AppError::Validation(_) | AppError::FieldValidation(_) => ErrorCode::Validation,
            AppError::Conflict(_) => ErrorCode::Conflict,
            AppError::AccountLocked(_) => ErrorCode::AccountLocked,
            AppError::StrictModeBlocked { .. } => ErrorCode::StrictModeBlocked,
            AppError::Internal(_) => ErrorCode::Internal,
            AppError::Database(_) => ErrorCode::Database,
        }
    }

    /// Public message. `Internal` and `Database` details are intentionally
    /// *not* leaked — the internal text is only logged, never returned.
    pub fn public_message(&self) -> String {
        match self {
            AppError::Internal(_) => "An internal server error occurred.".into(),
            AppError::Database(_) => "A database error occurred.".into(),
            other => other.to_string(),
        }
    }

    pub fn fields(&self) -> Option<HashMap<String, Vec<String>>> {
        match self {
            AppError::FieldValidation(f) => Some(f.clone()),
            _ => None,
        }
    }

    pub fn into_envelope(&self, request_id: String) -> ErrorEnvelope {
        ErrorEnvelope {
            error: ErrorBody {
                code: self.code(),
                message: self.public_message(),
                fields: self.fields(),
                request_id,
            },
        }
    }
}

/// Makes `AppError` directly returnable from Rocket handlers.
///
/// The request guard `RequestId` populates the correlation id (see
/// `api::guards`). If no guard ran (e.g. the error came from a pre-guard
/// fairing path), a fresh v4 UUID is generated.
impl<'r> Responder<'r, 'static> for AppError {
    fn respond_to(self, req: &'r Request<'_>) -> response::Result<'static> {
        let request_id = req
            .local_cache(|| crate::api::guards::RequestId(Uuid::new_v4().to_string()))
            .0
            .clone();

        // Log internal details server-side; never ship them to the client.
        match &self {
            AppError::Internal(detail) => tracing::error!(request_id = %request_id, "internal error: {}", detail),
            AppError::Database(detail) => tracing::error!(request_id = %request_id, "database error: {}", detail),
            AppError::Unauthorized => tracing::debug!(request_id = %request_id, "unauthorized"),
            AppError::Forbidden => tracing::debug!(request_id = %request_id, "forbidden"),
            other => tracing::debug!(request_id = %request_id, "client error: {}", other),
        }

        let envelope = self.into_envelope(request_id);
        let body = serde_json::to_string(&envelope).unwrap_or_else(|_| "{}".into());
        Response::build()
            .status(self.status())
            .header(ContentType::JSON)
            .sized_body(body.len(), Cursor::new(body))
            .ok()
    }
}

/// Convenience `Result` alias used everywhere.
pub type AppResult<T> = Result<T, AppError>;
