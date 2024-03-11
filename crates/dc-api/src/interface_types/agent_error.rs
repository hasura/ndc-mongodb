use std::fmt;

use axum::{
    extract::rejection::{JsonRejection, TypedHeaderRejection},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use thiserror::Error;

use dc_api_types::ErrorResponse;

/// Type for all errors that might occur as a result of requests sent to the agent.
#[derive(Debug, Error)]
pub enum AgentError {
    BadHeader(#[from] TypedHeaderRejection),
    BadJWT(#[from] jsonwebtoken::errors::Error),
    BadJWTNoKID,
    BadJSONRequestBody(#[from] JsonRejection),
    /// Default case for deserialization failures *not including* parsing request bodies.
    Deserialization(#[from] serde_json::Error),
    InvalidLicenseKey,
    NotFound(axum::http::Uri),
}

use AgentError::*;

impl AgentError {
    pub fn status_and_error_response(&self) -> (StatusCode, ErrorResponse) {
        match self {
            BadHeader(err) => (StatusCode::BAD_REQUEST, ErrorResponse::new(&err)),
            BadJWT(err) => (
                StatusCode::UNAUTHORIZED,
                ErrorResponse {
                    message: "Could not decode JWT".to_owned(),
                    details: Some(
                        [(
                            "error".to_owned(),
                            serde_json::Value::String(err.to_string()),
                        )]
                        .into(),
                    ),
                    r#type: None,
                },
            ),
            BadJWTNoKID => (
                StatusCode::UNAUTHORIZED,
                ErrorResponse::new("License Token doesn't have a `kid` header field"),
            ),
            BadJSONRequestBody(err) => (StatusCode::BAD_REQUEST, ErrorResponse::new(&err)),
            Deserialization(err) => (StatusCode::BAD_REQUEST, ErrorResponse::new(&err)),
            InvalidLicenseKey => (
                StatusCode::UNAUTHORIZED,
                ErrorResponse::new("Invalid License Key"),
            ),
            NotFound(uri) => (
                StatusCode::NOT_FOUND,
                ErrorResponse::new(&format!("No Route {uri}")),
            ),
        }
    }
}

impl fmt::Display for AgentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (_, err) = self.status_and_error_response();
        write!(f, "{}", err.message)
    }
}

impl IntoResponse for AgentError {
    fn into_response(self) -> axum::response::Response {
        if cfg!(debug_assertions) {
            // Log certain errors in development only. The `debug_assertions` feature is present in
            // debug builds, which we use during development. It is not present in release builds.
            match &self {
                BadHeader(err) => tracing::warn!(error = %err, "error reading rquest header"),
                BadJSONRequestBody(err) => {
                    tracing::warn!(error = %err, "error parsing request body")
                }
                InvalidLicenseKey => tracing::warn!("invalid license key"),
                _ => (),
            }
        }
        let (status, resp) = self.status_and_error_response();
        (status, Json(resp)).into_response()
    }
}
