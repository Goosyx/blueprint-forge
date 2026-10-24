//! PT: Proteção contra CSRF: toda requisição que muda estado precisa vir da origem configurada.
//!     Funciona junto com o cookie `SameSite=Strict`.
//! EN: CSRF protection: every state-changing request must come from the configured origin.
//!     It works together with the `SameSite=Strict` cookie.

use axum::extract::{Request, State};
use axum::http::{Method, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

use crate::error::ApiError;
use crate::state::AppState;

/// PT: Middleware que recusa POST/PUT/PATCH/DELETE sem `Origin` igual ao configurado.
/// EN: Middleware that refuses POST/PUT/PATCH/DELETE without an `Origin` equal to the configured one.
pub async fn check_origin(State(state): State<AppState>, request: Request, next: Next) -> Response {
    let safe = matches!(
        *request.method(),
        Method::GET | Method::HEAD | Method::OPTIONS
    );
    if !safe {
        let origin = request
            .headers()
            .get(header::ORIGIN)
            .and_then(|v| v.to_str().ok());
        if origin != Some(state.config.allowed_origin.as_str()) {
            return ApiError::Forbidden("origin.forbidden").into_response();
        }
    }
    next.run(request).await
}
