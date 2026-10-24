//! PT: Sessões: token de 32 bytes do gerador do sistema, guardado no cookie
//!     `__Host-bf_session` (HttpOnly, Secure, SameSite=Strict). O banco só guarda o SHA-256 do token.
//!     Expiração absoluta de 30 dias e por inatividade de 7 dias; no máximo `Quotas::sessions` por usuário.
//! EN: Sessions: a 32-byte token from the system generator, stored in the `__Host-bf_session`
//!     cookie (HttpOnly, Secure, SameSite=Strict). The database only stores the token's SHA-256.
//!     Absolute expiry of 30 days and idle expiry of 7 days; at most `Quotas::sessions` per user.

use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::http::{HeaderMap, HeaderValue, header};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use rand::TryRng;
use sha2::{Digest, Sha256};
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

use crate::error::{ApiError, ApiResult};
use crate::state::AppState;

/// PT: Nome do cookie; o prefixo `__Host-` obriga Secure, Path=/ e nenhum Domain.
/// EN: Cookie name; the `__Host-` prefix requires Secure, Path=/ and no Domain.
pub const COOKIE: &str = "__Host-bf_session";
const ABSOLUTE_LIFETIME: Duration = Duration::days(30);
const IDLE_LIFETIME: Duration = Duration::days(7);
/// PT: Intervalo mínimo entre atualizações de `last_seen_at`. EN: Minimum interval between `last_seen_at` updates.
const TOUCH_INTERVAL: Duration = Duration::minutes(5);

/// PT: Usuário logado, extraído do cookie em cada rota protegida.
/// EN: Logged-in user, extracted from the cookie on every protected route.
#[derive(Debug, Clone)]
pub struct CurrentUser {
    pub id: Uuid,
    pub email: String,
    pub locale: String,
    pub token_hash: Vec<u8>,
}

/// PT: Gera um token novo: devolve (token para o cookie, hash para o banco).
/// EN: Generates a new token: returns (token for the cookie, hash for the database).
pub fn new_token() -> ApiResult<(String, Vec<u8>)> {
    let mut bytes = [0u8; 32];
    rand::rngs::SysRng
        .try_fill_bytes(&mut bytes)
        .map_err(|_| ApiError::Internal)?;
    let token = URL_SAFE_NO_PAD.encode(bytes);
    let hash = token_hash(&token);
    Ok((token, hash))
}

/// PT: SHA-256 do token. EN: SHA-256 of the token.
pub fn token_hash(token: &str) -> Vec<u8> {
    Sha256::digest(token.as_bytes()).to_vec()
}

/// PT: Cabeçalho `Set-Cookie` da sessão. EN: Session `Set-Cookie` header.
pub fn set_cookie(token: &str) -> ApiResult<HeaderValue> {
    HeaderValue::from_str(&format!(
        "{COOKIE}={token}; Path=/; HttpOnly; Secure; SameSite=Strict; Max-Age={}",
        ABSOLUTE_LIFETIME.whole_seconds()
    ))
    .map_err(|_| ApiError::Internal)
}

/// PT: Cabeçalho que apaga o cookie. EN: Header that clears the cookie.
pub fn clear_cookie() -> HeaderValue {
    HeaderValue::from_static(
        "__Host-bf_session=; Path=/; HttpOnly; Secure; SameSite=Strict; Max-Age=0",
    )
}

// PT: Lê o token do cookie e confere o formato (43 caracteres base64url) antes de qualquer uso.
// EN: Reads the token from the cookie and checks its format (43 base64url characters) before any use.
fn token_from(headers: &HeaderMap) -> Option<String> {
    for value in headers.get_all(header::COOKIE) {
        let Ok(text) = value.to_str() else { continue };
        for pair in text.split(';') {
            let Some((name, token)) = pair.trim().split_once('=') else {
                continue;
            };
            if name == COOKIE
                && token.len() == 43
                && token
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
            {
                return Some(token.to_owned());
            }
        }
    }
    None
}

/// PT: Cria a sessão e apaga as mais antigas além do limite do usuário.
/// EN: Creates the session and deletes the oldest ones beyond the user's limit.
pub async fn create(state: &AppState, user: Uuid, user_agent: Option<&str>) -> ApiResult<String> {
    let (token, hash) = new_token()?;
    let now = OffsetDateTime::now_utc();
    let agent: Option<String> = user_agent.map(|agent| agent.chars().take(256).collect());
    let mut tx = state.db.begin().await?;
    sqlx::query!(
        "INSERT INTO sessions (token_hash, user_id, expires_at, user_agent) VALUES ($1, $2, $3, $4)",
        hash,
        user,
        now + ABSOLUTE_LIFETIME,
        agent
    )
    .execute(&mut *tx)
    .await?;
    sqlx::query!(
        "DELETE FROM sessions WHERE user_id = $1 AND token_hash IN (
             SELECT token_hash FROM sessions WHERE user_id = $1
             ORDER BY created_at DESC OFFSET $2)",
        user,
        state.config.quotas.sessions
    )
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(token)
}

/// PT: Apaga uma sessão. EN: Deletes one session.
pub async fn delete(state: &AppState, token_hash: &[u8]) -> ApiResult<()> {
    sqlx::query!("DELETE FROM sessions WHERE token_hash = $1", token_hash)
        .execute(&state.db)
        .await?;
    Ok(())
}

impl FromRequestParts<AppState> for CurrentUser {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let token = token_from(&parts.headers).ok_or(ApiError::Unauthorized)?;
        let hash = token_hash(&token);
        let now = OffsetDateTime::now_utc();
        let row = sqlx::query!(
            "SELECT s.user_id, s.last_seen_at, u.email, u.locale
             FROM sessions s JOIN users u ON u.id = s.user_id
             WHERE s.token_hash = $1 AND s.expires_at > $2 AND s.last_seen_at > $3",
            hash,
            now,
            now - IDLE_LIFETIME
        )
        .fetch_optional(&state.db)
        .await?
        .ok_or(ApiError::Unauthorized)?;

        if now - row.last_seen_at > TOUCH_INTERVAL {
            sqlx::query!(
                "UPDATE sessions SET last_seen_at = $1 WHERE token_hash = $2",
                now,
                hash
            )
            .execute(&state.db)
            .await?;
        }
        Ok(Self {
            id: row.user_id,
            email: row.email,
            locale: row.locale,
            token_hash: hash,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_are_random_and_cookie_parsing_is_strict() {
        let (a, hash_a) = new_token().unwrap();
        let (b, _) = new_token().unwrap();
        assert_ne!(a, b);
        assert_eq!(a.len(), 43);
        assert_eq!(hash_a.len(), 32);

        let mut headers = HeaderMap::new();
        headers.insert(
            header::COOKIE,
            HeaderValue::from_str(&format!("x=1; {COOKIE}={a}")).unwrap(),
        );
        assert_eq!(token_from(&headers), Some(a));

        let mut bad = HeaderMap::new();
        bad.insert(
            header::COOKIE,
            HeaderValue::from_static("__Host-bf_session=short"),
        );
        assert_eq!(token_from(&bad), None);
        let mut injected = HeaderMap::new();
        injected.insert(
            header::COOKIE,
            HeaderValue::from_static(
                "__Host-bf_session=' OR 1=1 --aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            ),
        );
        assert_eq!(token_from(&injected), None);
    }
}
