//! PT: Rotas de dados do usuário. Todas filtram por `user_id`: recurso de outro usuário é 404.
//! EN: User data routes. All of them filter by `user_id`: another user's resource is a 404.

pub mod analyze;
pub mod blueprints;
pub mod preferences;
pub mod projects;

use serde::Deserialize;

/// PT: Paginação por cursor: `limit` até 100 e o último id visto.
/// EN: Cursor pagination: `limit` up to 100 and the last seen id.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Page {
    #[serde(default)]
    pub limit: Option<i64>,
    #[serde(default)]
    pub after: Option<uuid::Uuid>,
}

impl Page {
    /// PT: Tamanho da página entre 1 e 100 (padrão 50). EN: Page size between 1 and 100 (default 50).
    pub fn limit(&self) -> i64 {
        self.limit.unwrap_or(50).clamp(1, 100)
    }
}

/// PT: Texto opcional aparado, vazio vira `None`, com limite de caracteres.
/// EN: Optional trimmed text, empty becomes `None`, with a character limit.
pub fn clean_text(
    text: Option<&str>,
    max: usize,
    code: &'static str,
) -> crate::error::ApiResult<Option<String>> {
    let Some(text) = text.map(str::trim).filter(|t| !t.is_empty()) else {
        return Ok(None);
    };
    if text.chars().count() > max || text.chars().any(|c| c.is_control() && c != '\n') {
        return Err(crate::error::ApiError::BadRequest(code));
    }
    Ok(Some(text.to_owned()))
}
