// Error types and gRPC→HTTP status mapping.
// The mapping rules mirror Python's exceptions.py grpc_to_http_status exactly.

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;
use thiserror::Error;
use tonic::Code;

#[derive(Debug, Error)]
pub enum AppError {
    /// gRPC transport or RPC error from xray node
    #[error("gRPC error: {code:?} — {details}")]
    Grpc { code: Code, details: String },

    /// Missing or invalid Authorization header (→ 401)
    #[error("Unauthenticated: {0}")]
    Unauthenticated(String),

    /// Missing X-Node-Domain / X-Node-Token header (→ 400)
    #[error("Invalid argument: {0}")]
    InvalidArgument(String),

    /// Request body validation failure (→ 422)
    #[error("Unprocessable entity: {0}")]
    UnprocessableEntity(String),

    /// Unexpected internal error (→ 500)
    #[error("Internal error: {0}")]
    Internal(String),
}

impl From<tonic::Status> for AppError {
    fn from(status: tonic::Status) -> Self {
        Self::Grpc {
            code: status.code(),
            details: status.message().to_owned(),
        }
    }
}

/// Convert gRPC status code + details to HTTP status code.
/// Matches Python's grpc_to_http_status logic exactly.
pub fn grpc_code_to_http(code: Code, details: &str) -> StatusCode {
    match code {
        Code::NotFound => StatusCode::NOT_FOUND,
        Code::AlreadyExists => StatusCode::CONFLICT,
        Code::Unimplemented => StatusCode::NOT_IMPLEMENTED,
        Code::Unavailable => StatusCode::BAD_GATEWAY,
        Code::Unknown => {
            let lower = details.to_lowercase();
            if lower.contains("not found") {
                StatusCode::NOT_FOUND
            } else if lower.contains("already exists") {
                StatusCode::CONFLICT
            } else {
                StatusCode::BAD_GATEWAY
            }
        }
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

/// Serialize a tonic::Code to SCREAMING_SNAKE_CASE string as Python does.
fn code_name(code: Code) -> &'static str {
    match code {
        Code::Ok => "OK",
        Code::Cancelled => "CANCELLED",
        Code::Unknown => "UNKNOWN",
        Code::InvalidArgument => "INVALID_ARGUMENT",
        Code::DeadlineExceeded => "DEADLINE_EXCEEDED",
        Code::NotFound => "NOT_FOUND",
        Code::AlreadyExists => "ALREADY_EXISTS",
        Code::PermissionDenied => "PERMISSION_DENIED",
        Code::ResourceExhausted => "RESOURCE_EXHAUSTED",
        Code::FailedPrecondition => "FAILED_PRECONDITION",
        Code::Aborted => "ABORTED",
        Code::OutOfRange => "OUT_OF_RANGE",
        Code::Unimplemented => "UNIMPLEMENTED",
        Code::Internal => "INTERNAL",
        Code::Unavailable => "UNAVAILABLE",
        Code::DataLoss => "DATA_LOSS",
        Code::Unauthenticated => "UNAUTHENTICATED",
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        match self {
            AppError::Grpc { code, details } => {
                let http_status = grpc_code_to_http(code, &details);
                let body = json!({
                    "ok": false,
                    "code": code_name(code),
                    "error": details,
                });
                (http_status, Json(body)).into_response()
            }
            AppError::Unauthenticated(msg) => {
                let body = json!({
                    "ok": false,
                    "code": "UNAUTHENTICATED",
                    "error": msg,
                });
                (StatusCode::UNAUTHORIZED, Json(body)).into_response()
            }
            AppError::InvalidArgument(msg) => {
                let body = json!({
                    "ok": false,
                    "code": "INVALID_ARGUMENT",
                    "error": msg,
                });
                (StatusCode::BAD_REQUEST, Json(body)).into_response()
            }
            AppError::UnprocessableEntity(msg) => {
                let body = json!({
                    "ok": false,
                    "code": "INVALID_ARGUMENT",
                    "error": msg,
                });
                (StatusCode::UNPROCESSABLE_ENTITY, Json(body)).into_response()
            }
            AppError::Internal(msg) => {
                let body = json!({
                    "ok": false,
                    "code": "INTERNAL_ERROR",
                    "error": msg,
                });
                (StatusCode::INTERNAL_SERVER_ERROR, Json(body)).into_response()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grpc_to_http_not_found() {
        assert_eq!(grpc_code_to_http(Code::NotFound, ""), StatusCode::NOT_FOUND);
    }

    #[test]
    fn grpc_to_http_already_exists() {
        assert_eq!(
            grpc_code_to_http(Code::AlreadyExists, ""),
            StatusCode::CONFLICT
        );
    }

    #[test]
    fn grpc_to_http_unimplemented() {
        assert_eq!(
            grpc_code_to_http(Code::Unimplemented, ""),
            StatusCode::NOT_IMPLEMENTED
        );
    }

    #[test]
    fn grpc_to_http_unavailable() {
        assert_eq!(
            grpc_code_to_http(Code::Unavailable, ""),
            StatusCode::BAD_GATEWAY
        );
    }

    #[test]
    fn grpc_to_http_unknown_not_found() {
        assert_eq!(
            grpc_code_to_http(Code::Unknown, "user not found in database"),
            StatusCode::NOT_FOUND
        );
    }

    #[test]
    fn grpc_to_http_unknown_already_exists() {
        assert_eq!(
            grpc_code_to_http(Code::Unknown, "already exists: user"),
            StatusCode::CONFLICT
        );
    }

    #[test]
    fn grpc_to_http_unknown_other() {
        assert_eq!(
            grpc_code_to_http(Code::Unknown, "connection reset"),
            StatusCode::BAD_GATEWAY
        );
    }

    #[test]
    fn grpc_to_http_other_codes_are_500() {
        assert_eq!(
            grpc_code_to_http(Code::Internal, ""),
            StatusCode::INTERNAL_SERVER_ERROR
        );
        assert_eq!(
            grpc_code_to_http(Code::Cancelled, ""),
            StatusCode::INTERNAL_SERVER_ERROR
        );
        assert_eq!(
            grpc_code_to_http(Code::PermissionDenied, ""),
            StatusCode::INTERNAL_SERVER_ERROR
        );
    }

    #[test]
    fn grpc_to_http_unknown_case_insensitive() {
        // "not found" must match case-insensitively as Python does
        assert_eq!(
            grpc_code_to_http(Code::Unknown, "NOT FOUND"),
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            grpc_code_to_http(Code::Unknown, "Not Found"),
            StatusCode::NOT_FOUND
        );
    }
}
