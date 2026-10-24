//! PT: Erros da API no formato `{"error":{"code":"...","message":"..."}}`. O `code` é a chave de
//!     tradução da interface; a mensagem nunca traz detalhes internos nem texto do usuário.
//! EN: API errors in the `{"error":{"code":"...","message":"..."}}` format. The `code` is the
//!     interface translation key; the message never carries internal details or user text.

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::json;

/// PT: Erro devolvido pela API.
/// EN: Error returned by the API.
#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("não encontrado / not found")]
    NotFound,
    #[error("requisição inválida / bad request: {0}")]
    BadRequest(&'static str),
    #[error("login necessário / login required")]
    Unauthorized,
    #[error("proibido / forbidden: {0}")]
    Forbidden(&'static str),
    #[error("conflito / conflict: {0}")]
    Conflict(&'static str),
    #[error("muitas requisições / too many requests")]
    TooManyRequests,
    #[error("serviço indisponível / service unavailable: {0}")]
    Unavailable(&'static str),
    #[error("erro interno / internal error")]
    Internal,
}

impl ApiError {
    fn status_and_code(&self) -> (StatusCode, &'static str) {
        match self {
            Self::NotFound => (StatusCode::NOT_FOUND, "not_found"),
            Self::BadRequest(code) => (StatusCode::BAD_REQUEST, code),
            Self::Unauthorized => (StatusCode::UNAUTHORIZED, "auth.required"),
            Self::Forbidden(code) => (StatusCode::FORBIDDEN, code),
            Self::Conflict(code) => (StatusCode::CONFLICT, code),
            Self::TooManyRequests => (StatusCode::TOO_MANY_REQUESTS, "rate_limited"),
            Self::Unavailable(code) => (StatusCode::SERVICE_UNAVAILABLE, code),
            Self::Internal => (StatusCode::INTERNAL_SERVER_ERROR, "internal"),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, code) = self.status_and_code();
        (
            status,
            Json(json!({ "error": { "code": code, "message": self.to_string() } })),
        )
            .into_response()
    }
}

// PT: Erro do banco vira 500 genérico; o detalhe só vai para o log do servidor.
// EN: A database error becomes a generic 500; the detail only goes to the server log.
impl From<sqlx::Error> for ApiError {
    fn from(error: sqlx::Error) -> Self {
        tracing::error!(error = %error, "erro de banco / database error");
        Self::Internal
    }
}

/// PT: Atalho para resultados da API. EN: Shortcut for API results.
pub type ApiResult<T> = Result<T, ApiError>;
