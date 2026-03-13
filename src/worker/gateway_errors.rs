//! Gateway error helpers and HTTP status code mapping.

use axum::{http::StatusCode, Json};

use super::gateway_types::ErrorResponse;

pub type GatewayError = (StatusCode, Json<ErrorResponse>);

pub fn bad_request(msg: impl std::fmt::Display, code: &str) -> GatewayError {
    (
        StatusCode::BAD_REQUEST,
        Json(ErrorResponse {
            error: msg.to_string(),
            code: code.to_string(),
            error_id: None,
            details: None,
        }),
    )
}

pub fn map_init_error(e: crate::types::Error) -> GatewayError {
    let (status, code) = match &e {
        crate::types::Error::QuotaExceeded(_) => (StatusCode::TOO_MANY_REQUESTS, "RATE_LIMITED"),
        _ => (StatusCode::INTERNAL_SERVER_ERROR, "INIT_FAILED"),
    };
    let error_id = uuid::Uuid::new_v4().to_string();
    (
        status,
        Json(ErrorResponse {
            error: e.to_string(),
            code: code.to_string(),
            error_id: Some(error_id),
            details: None,
        }),
    )
}

pub fn map_pipeline_error(e: crate::types::Error) -> GatewayError {
    let error_id = uuid::Uuid::new_v4().to_string();
    let (status, code, details) = match &e {
        crate::types::Error::Validation { message, .. } => (
            StatusCode::BAD_REQUEST,
            "INVALID_ARGUMENT",
            Some(serde_json::json!({ "validation_message": message })),
        ),
        crate::types::Error::NotFound(_) => (StatusCode::NOT_FOUND, "NOT_FOUND", None),
        crate::types::Error::QuotaExceeded(msg) => (
            StatusCode::TOO_MANY_REQUESTS,
            "RATE_LIMITED",
            Some(serde_json::json!({ "limit_info": msg })),
        ),
        crate::types::Error::StateTransition(_) => (StatusCode::CONFLICT, "FAILED_PRECONDITION", None),
        crate::types::Error::Timeout(_) => (StatusCode::GATEWAY_TIMEOUT, "TIMEOUT", None),
        crate::types::Error::Cancelled(_) => (StatusCode::BAD_REQUEST, "CANCELLED", None),
        _ => (StatusCode::INTERNAL_SERVER_ERROR, "INTERNAL", None),
    };
    (
        status,
        Json(ErrorResponse {
            error: e.to_string(),
            code: code.to_string(),
            error_id: Some(error_id),
            details,
        }),
    )
}
