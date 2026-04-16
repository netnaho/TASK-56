//! Unit tests for `crate::api::client::ApiError` — covers the error type
//! invariants used by all frontend API client modules to classify backend
//! responses and drive UI error handling (e.g., redirect to login on 401).

use crate::api::client::ApiError;

// ---------------------------------------------------------------------------
// Constructor helpers
// ---------------------------------------------------------------------------

#[test]
fn network_error_has_zero_status_and_network_error_code() {
    let e = ApiError::network("connection refused");
    assert_eq!(e.status, 0);
    assert_eq!(e.code, "network_error");
    assert_eq!(e.message, "connection refused");
}

#[test]
fn decode_error_has_zero_status_and_decode_error_code() {
    let e = ApiError::decode("invalid JSON");
    assert_eq!(e.status, 0);
    assert_eq!(e.code, "decode_error");
    assert_eq!(e.message, "invalid JSON");
}

// ---------------------------------------------------------------------------
// is_unauthorized()
// ---------------------------------------------------------------------------

#[test]
fn http_401_is_unauthorized() {
    let e = ApiError {
        code: "unauthorized".to_string(),
        message: "Auth required".to_string(),
        status: 401,
    };
    assert!(e.is_unauthorized());
}

#[test]
fn unauthorized_code_without_401_status_is_also_unauthorized() {
    let e = ApiError {
        code: "unauthorized".to_string(),
        message: "session expired".to_string(),
        status: 0, // network-level failure that resolved to 401
    };
    assert!(e.is_unauthorized());
}

#[test]
fn http_403_is_not_unauthorized() {
    let e = ApiError {
        code: "forbidden".to_string(),
        message: "Insufficient permissions".to_string(),
        status: 403,
    };
    assert!(!e.is_unauthorized());
}

// ---------------------------------------------------------------------------
// is_forbidden()
// ---------------------------------------------------------------------------

#[test]
fn http_403_is_forbidden() {
    let e = ApiError {
        code: "forbidden".to_string(),
        message: "Insufficient permissions".to_string(),
        status: 403,
    };
    assert!(e.is_forbidden());
}

#[test]
fn forbidden_code_without_403_status_is_also_forbidden() {
    let e = ApiError {
        code: "forbidden".to_string(),
        message: "dept scope mismatch".to_string(),
        status: 0,
    };
    assert!(e.is_forbidden());
}

#[test]
fn http_401_is_not_forbidden() {
    let e = ApiError {
        code: "unauthorized".to_string(),
        message: "Auth required".to_string(),
        status: 401,
    };
    assert!(!e.is_forbidden());
}

// ---------------------------------------------------------------------------
// Display
// ---------------------------------------------------------------------------

#[test]
fn display_formats_code_status_and_message() {
    let e = ApiError {
        code: "not_found".to_string(),
        message: "Journal not found".to_string(),
        status: 404,
    };
    let s = format!("{}", e);
    assert!(s.contains("not_found"), "Display must include code; got: {s}");
    assert!(s.contains("404"), "Display must include status; got: {s}");
    assert!(s.contains("Journal not found"), "Display must include message; got: {s}");
}

// ---------------------------------------------------------------------------
// Clone and PartialEq
// ---------------------------------------------------------------------------

#[test]
fn api_error_clone_equals_original() {
    let e = ApiError {
        code: "validation".to_string(),
        message: "field required".to_string(),
        status: 422,
    };
    assert_eq!(e.clone(), e);
}

#[test]
fn different_api_errors_are_not_equal() {
    let e1 = ApiError {
        code: "not_found".to_string(),
        message: "not found".to_string(),
        status: 404,
    };
    let e2 = ApiError {
        code: "unauthorized".to_string(),
        message: "not authorized".to_string(),
        status: 401,
    };
    assert_ne!(e1, e2);
}
