//! [`CloudError`] — the unified error type for Kainetic Cloud.
//!
//! Implements [`axum::response::IntoResponse`] so it can be returned directly
//! from route handlers.

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;
use thiserror::Error;

/// All errors that can originate in the Kainetic Cloud backend.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum CloudError {
    /// Database I/O failure.
    #[error("database error: {0}")]
    Database(String),

    /// The requested resource was not found.
    #[error("not found: {0}")]
    NotFound(String),

    /// The request was malformed or failed validation.
    #[error("bad request: {0}")]
    BadRequest(String),

    /// Authentication failed (missing or invalid credentials).
    #[error("unauthorized: {0}")]
    Unauthorized(String),

    /// The caller lacks permission for the requested operation.
    #[error("forbidden: {0}")]
    Forbidden(String),

    /// The request conflicts with existing state (e.g., resource already exists).
    #[error("conflict: {0}")]
    Conflict(String),

    /// An internal invariant was violated.
    #[error("internal error: {0}")]
    Internal(String),
}

impl CloudError {
    fn status_code(&self) -> StatusCode {
        match self {
            Self::Database(_) | Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::NotFound(_) => StatusCode::NOT_FOUND,
            Self::BadRequest(_) => StatusCode::BAD_REQUEST,
            Self::Unauthorized(_) => StatusCode::UNAUTHORIZED,
            Self::Forbidden(_) => StatusCode::FORBIDDEN,
            Self::Conflict(_) => StatusCode::CONFLICT,
        }
    }
}

impl IntoResponse for CloudError {
    fn into_response(self) -> Response {
        let status = self.status_code();
        let body = Json(json!({ "error": self.to_string() }));
        (status, body).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn database_is_500() {
        let e = CloudError::Database("conn refused".into());
        assert_eq!(e.status_code(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[test]
    fn not_found_is_404() {
        let e = CloudError::NotFound("agent xyz".into());
        assert_eq!(e.status_code(), StatusCode::NOT_FOUND);
    }

    #[test]
    fn unauthorized_is_401() {
        let e = CloudError::Unauthorized("missing token".into());
        assert_eq!(e.status_code(), StatusCode::UNAUTHORIZED);
    }

    #[test]
    fn forbidden_is_403() {
        let e = CloudError::Forbidden("admin only".into());
        assert_eq!(e.status_code(), StatusCode::FORBIDDEN);
    }

    #[test]
    fn bad_request_is_400() {
        let e = CloudError::BadRequest("invalid json".into());
        assert_eq!(e.status_code(), StatusCode::BAD_REQUEST);
    }
}
