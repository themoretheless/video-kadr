//! Thin HTTP helpers shared across route modules.

use crate::error::AppError;
use crate::messages;

pub async fn api_not_found_handler() -> AppError {
    AppError::not_found(messages::API_ROUTE_NOT_FOUND)
}

pub async fn method_not_allowed_handler() -> AppError {
    AppError::method_not_allowed(messages::METHOD_NOT_ALLOWED)
}
