//! PT: Hash de senha com Argon2id (19 MiB, 2 passadas, 1 thread), sempre fora da thread
//!     assíncrona e atrás de um semáforo, para que muitos logins não esgotem CPU e memória.
//! EN: Password hashing with Argon2id (19 MiB, 2 passes, 1 thread), always off the async thread
//!     and behind a semaphore, so many logins cannot exhaust CPU and memory.

use argon2::password_hash::phc::PasswordHash;
use argon2::password_hash::{PasswordHasher, PasswordVerifier};
use argon2::{Algorithm, Argon2, Params, Version};

use crate::error::ApiError;
use crate::state::AppState;

/// PT: Tamanho mínimo e máximo da senha, em caracteres. EN: Minimum and maximum password size, in characters.
pub const MIN_PASSWORD: usize = 10;
pub const MAX_PASSWORD: usize = 128;

fn hasher() -> Result<Argon2<'static>, ApiError> {
    let params = Params::new(19 * 1024, 2, 1, None).map_err(|_| ApiError::Internal)?;
    Ok(Argon2::new(Algorithm::Argon2id, Version::V0x13, params))
}

/// PT: Confere o tamanho da senha. EN: Checks the password size.
pub fn validate(password: &str) -> Result<(), ApiError> {
    let length = password.chars().count();
    if !(MIN_PASSWORD..=MAX_PASSWORD).contains(&length) {
        return Err(ApiError::BadRequest("auth.password_length"));
    }
    Ok(())
}

/// PT: Gera o hash PHC (sem semáforo; uso na linha de comando e na inicialização).
/// EN: Produces the PHC hash (no semaphore; for the command line and startup).
pub fn hash_now(password: &str) -> Result<String, ApiError> {
    hasher()?
        .hash_password(password.as_bytes())
        .map(|hash| hash.to_string())
        .map_err(|_| ApiError::Internal)
}

/// PT: Espera por uma vaga de hash no máximo 5 s; depois disso responde 503 em vez de enfileirar
///     sem fim (protege o login de todos contra uma enxurrada de pedidos).
/// EN: Waits for a hashing slot for at most 5 s; after that it answers 503 instead of queueing
///     forever (protects everyone's login against a flood of requests).
async fn acquire(state: &AppState) -> Result<tokio::sync::SemaphorePermit<'_>, ApiError> {
    tokio::time::timeout(
        std::time::Duration::from_secs(5),
        state.hash_slots.acquire(),
    )
    .await
    .map_err(|_| ApiError::Unavailable("auth.busy"))?
    .map_err(|_| ApiError::Internal)
}

/// PT: Gera o hash numa thread de bloqueio, respeitando o semáforo.
/// EN: Produces the hash on a blocking thread, respecting the semaphore.
pub async fn hash(state: &AppState, password: String) -> Result<String, ApiError> {
    let _permit = acquire(state).await?;
    tokio::task::spawn_blocking(move || hash_now(&password))
        .await
        .map_err(|_| ApiError::Internal)?
}

/// PT: Confere a senha contra o hash numa thread de bloqueio, respeitando o semáforo.
/// EN: Verifies the password against the hash on a blocking thread, respecting the semaphore.
pub async fn verify(state: &AppState, password: String, hash: String) -> Result<bool, ApiError> {
    let _permit = acquire(state).await?;
    tokio::task::spawn_blocking(move || {
        let parsed = PasswordHash::new(&hash).map_err(|_| ApiError::Internal)?;
        Ok(hasher()?
            .verify_password(password.as_bytes(), &parsed)
            .is_ok())
    })
    .await
    .map_err(|_| ApiError::Internal)?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hashes_and_validates() {
        let hash = hash_now("correct horse battery").unwrap();
        assert!(hash.starts_with("$argon2id$v=19$m=19456,t=2,p=1$"));
        assert!(validate("short").is_err());
        assert!(validate(&"x".repeat(129)).is_err());
        assert!(validate("long enough password").is_ok());
    }
}
