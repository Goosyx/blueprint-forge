//! PT: Tarefas periódicas: apaga sessões e convites vencidos e confere o tamanho do banco. Acima de
//!     90% do teto global (`BF_MAX_DB_BYTES`), o servidor entra em modo somente leitura.
//! EN: Periodic tasks: deletes expired sessions and invites and checks the database size. Above 90%
//!     of the global cap (`BF_MAX_DB_BYTES`), the server switches to read-only mode.

use std::sync::atomic::Ordering;
use std::time::Duration;

use crate::state::AppState;

const INTERVAL: Duration = Duration::from_secs(300);

/// PT: Uma rodada de manutenção. EN: One maintenance round.
pub async fn run_once(state: &AppState) -> Result<(), sqlx::Error> {
    // PT: Esquece chaves antigas dos limitadores, para a memória não crescer com IPs novos.
    // EN: Forgets old rate limiter keys, so memory does not grow with new IPs.
    state.limiters.login.retain_recent();
    state.limiters.register.retain_recent();
    state.limiters.writes.retain_recent();
    sqlx::query!(
        "DELETE FROM sessions WHERE expires_at < now() OR last_seen_at < now() - interval '7 days'"
    )
    .execute(&state.db)
    .await?;
    sqlx::query!("DELETE FROM invites WHERE expires_at < now() AND used_by IS NULL")
        .execute(&state.db)
        .await?;
    let size = sqlx::query_scalar!(r#"SELECT pg_database_size(current_database()) AS "size!""#)
        .fetch_one(&state.db)
        .await?;
    let limit = state.config.max_db_bytes.saturating_mul(9) / 10;
    let full = size > limit;
    let was_full = state.read_only.swap(full, Ordering::Relaxed);
    if full && !was_full {
        tracing::warn!(
            size,
            limit,
            "banco perto do teto: somente leitura / database near cap: read-only"
        );
    } else if !full && was_full {
        tracing::info!(
            size,
            "banco abaixo do teto: gravação liberada / database below cap: writes allowed"
        );
    }
    Ok(())
}

/// PT: Laço de manutenção em segundo plano. EN: Background maintenance loop.
pub async fn run_forever(state: AppState) {
    loop {
        if let Err(error) = run_once(&state).await {
            tracing::error!(error = %error, "falha na manutenção / maintenance failed");
        }
        tokio::time::sleep(INTERVAL).await;
    }
}
