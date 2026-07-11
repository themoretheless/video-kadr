//! Stable HTTP error contract for every API endpoint.

use axum::extract::{FromRequest, Multipart, Request};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::de::DeserializeOwned;
use serde::Serialize;

pub type AppResult<T> = Result<T, AppError>;

#[derive(Debug)]
pub struct AppError {
    status: StatusCode,
    code: &'static str,
    message: String,
    internal: Option<InternalError>,
}

#[derive(Debug)]
struct InternalError {
    context: &'static str,
    source: anyhow::Error,
}

#[derive(Debug, Serialize)]
struct ApiErrorBody {
    error: String,
    code: &'static str,
}

impl AppError {
    fn client(status: StatusCode, code: &'static str, message: impl Into<String>) -> Self {
        Self {
            status,
            code,
            message: message.into(),
            internal: None,
        }
    }

    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::client(StatusCode::BAD_REQUEST, "bad_request", message)
    }

    pub fn invalid_json() -> Self {
        Self::client(
            StatusCode::BAD_REQUEST,
            "invalid_json",
            "Некорректное JSON-тело запроса",
        )
    }

    pub fn invalid_multipart() -> Self {
        Self::client(
            StatusCode::BAD_REQUEST,
            "invalid_multipart",
            "Некорректный multipart-запрос",
        )
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::client(StatusCode::NOT_FOUND, "not_found", message)
    }

    pub fn conflict(message: impl Into<String>) -> Self {
        Self::client(StatusCode::CONFLICT, "conflict", message)
    }

    pub fn method_not_allowed(message: impl Into<String>) -> Self {
        Self::client(
            StatusCode::METHOD_NOT_ALLOWED,
            "method_not_allowed",
            message,
        )
    }

    pub fn payload_too_large(message: impl Into<String>) -> Self {
        Self::client(StatusCode::PAYLOAD_TOO_LARGE, "payload_too_large", message)
    }

    pub fn too_many_requests(message: impl Into<String>) -> Self {
        Self::client(StatusCode::TOO_MANY_REQUESTS, "too_many_requests", message)
    }

    pub fn unsupported_media_type(message: impl Into<String>) -> Self {
        Self::client(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "unsupported_media_type",
            message,
        )
    }

    pub fn request_timeout(message: impl Into<String>) -> Self {
        Self::client(StatusCode::REQUEST_TIMEOUT, "request_timeout", message)
    }

    pub fn gateway_timeout(message: impl Into<String>) -> Self {
        Self::client(StatusCode::GATEWAY_TIMEOUT, "gateway_timeout", message)
    }

    pub fn internal(context: &'static str, source: impl Into<anyhow::Error>) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            code: "internal_error",
            message: "Внутренняя ошибка сервера".into(),
            internal: Some(InternalError {
                context,
                source: source.into(),
            }),
        }
    }

    #[cfg(test)]
    pub(crate) fn status(&self) -> StatusCode {
        self.status
    }

    #[cfg(test)]
    pub(crate) fn code(&self) -> &'static str {
        self.code
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        if let Some(internal) = &self.internal {
            tracing::error!(
                context = internal.context,
                error = %internal.source,
                "API request failed"
            );
        }

        (
            self.status,
            Json(ApiErrorBody {
                error: self.message,
                code: self.code,
            }),
        )
            .into_response()
    }
}

/// JSON extractor whose rejection also follows the public API error contract.
pub struct ApiJson<T>(pub T);

#[axum::async_trait]
impl<T, S> FromRequest<S> for ApiJson<T>
where
    T: DeserializeOwned,
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request(req: Request, state: &S) -> AppResult<Self> {
        match Json::<T>::from_request(req, state).await {
            Ok(Json(value)) => Ok(Self(value)),
            Err(error) if error.status() == StatusCode::PAYLOAD_TOO_LARGE => Err(
                AppError::payload_too_large("JSON-тело запроса превышает допустимый размер"),
            ),
            Err(error) => {
                tracing::debug!(error = %error, "rejected JSON request");
                Err(AppError::invalid_json())
            }
        }
    }
}

/// Multipart extractor whose initial boundary errors use the same envelope.
pub struct ApiMultipart(pub Multipart);

#[axum::async_trait]
impl<S> FromRequest<S> for ApiMultipart
where
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request(req: Request, state: &S) -> AppResult<Self> {
        Multipart::from_request(req, state)
            .await
            .map(Self)
            .map_err(|error| {
                tracing::debug!(error = %error, "rejected multipart request");
                AppError::invalid_multipart()
            })
    }
}

#[cfg(test)]
mod tests {
    use axum::body::to_bytes;
    use serde_json::Value;

    use super::*;

    #[tokio::test]
    async fn internal_error_hides_its_source_from_the_response() {
        let response = AppError::internal("test database call", anyhow::anyhow!("secret path"))
            .into_response();
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);

        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let body: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(body["code"], "internal_error");
        assert_eq!(body["error"], "Внутренняя ошибка сервера");
        assert!(!String::from_utf8_lossy(&bytes).contains("secret path"));
    }
}
