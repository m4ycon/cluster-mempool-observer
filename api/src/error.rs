use crate::db::repositories::RepoError;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use observer::error::ObserverError;
use std::fmt::Display;

#[derive(Debug)]
pub enum ApiError {
    BadRequest(String),
    Internal(String),
}

impl Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ApiError::BadRequest(msg) => write!(f, "Bad request: {}", msg),
            ApiError::Internal(msg) => write!(f, "Internal error: {}", msg),
        }
    }
}

impl std::error::Error for ApiError {}

impl From<RepoError> for ApiError {
    fn from(err: RepoError) -> Self {
        ApiError::Internal(err.to_string())
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        match self {
            ApiError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg).into_response(),
            ApiError::Internal(msg) => {
                // Never return `msg` to the client: it can carry database error text.
                tracing::error!("internal error: {msg}");
                (StatusCode::INTERNAL_SERVER_ERROR, "internal error").into_response()
            }
        }
    }
}

#[derive(Debug)]
pub enum BlockSyncError {
    Node(ObserverError),
    Db(RepoError),
}

impl std::fmt::Display for BlockSyncError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BlockSyncError::Node(e) => write!(f, "node error: {e}"),
            BlockSyncError::Db(e) => write!(f, "db error: {e}"),
        }
    }
}

impl std::error::Error for BlockSyncError {}

impl From<ObserverError> for BlockSyncError {
    fn from(e: ObserverError) -> Self {
        BlockSyncError::Node(e)
    }
}

impl From<RepoError> for BlockSyncError {
    fn from(e: RepoError) -> Self {
        BlockSyncError::Db(e)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn bad_request_response_carries_the_message() {
        let response = ApiError::BadRequest("bad range".to_string()).into_response();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(body, "bad range".as_bytes());
    }

    #[tokio::test]
    async fn internal_response_does_not_leak_the_payload() {
        let response =
            ApiError::Internal("password=hunter2 host=127.0.0.1".to_string()).into_response();
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let body = String::from_utf8(body.to_vec()).unwrap();
        assert!(!body.contains("hunter2"));
        assert!(!body.contains("127.0.0.1"));
    }
}
