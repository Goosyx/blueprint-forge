//! PT: Rotas de conta: cadastro (fechado, por convite ou aberto), login com bloqueio por tentativas
//!     e tempo constante, logout, dados do usuário e troca de senha (que encerra as outras sessões).
//! EN: Account routes: registration (closed, by invite or open), login with attempt lockout and
//!     constant time, logout, user data and password change (which ends the other sessions).

use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use axum::Json;
use axum::extract::{ConnectInfo, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth::password;
use crate::auth::session::{self, CurrentUser};
use crate::config::Registration;
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;

/// PT: Tentativas erradas antes do bloqueio. EN: Wrong attempts before lockout.
const MAX_FAILED_LOGINS: i32 = 5;
/// PT: Duração do bloqueio. EN: Lockout duration.
const LOCKOUT_INTERVAL: sqlx::postgres::types::PgInterval = sqlx::postgres::types::PgInterval {
    months: 0,
    days: 0,
    microseconds: 15 * 60 * 1_000_000,
};

/// PT: E-mail normalizado (minúsculo, sem espaços nas pontas) e validado.
/// EN: Normalized (lowercase, trimmed) and validated email.
pub fn normalize_email(email: &str) -> ApiResult<String> {
    let email = email.trim().to_lowercase();
    let valid = (3..=254).contains(&email.chars().count())
        && email.split('@').count() == 2
        && !email.starts_with('@')
        && !email.ends_with('@')
        && !email.chars().any(|c| c.is_whitespace() || c.is_control());
    if !valid {
        return Err(ApiError::BadRequest("auth.invalid_email"));
    }
    Ok(email)
}

/// PT: IP do cliente para os limitadores. Vem da conexão; só se a conexão for do proxy confiável
///     (`BF_TRUSTED_PROXY`) é que o último endereço de `X-Forwarded-For` é usado. IPv6 é agrupado
///     por /64, para que um único bloco não gere chaves infinitas. Sem informação de conexão
///     (testes), vira 0.0.0.0.
/// EN: Client IP for the rate limiters. It comes from the connection; only if the connection is
///     from the trusted proxy (`BF_TRUSTED_PROXY`) is the last `X-Forwarded-For` address used. IPv6
///     is grouped by /64, so a single block cannot create endless keys. Without connection info
///     (tests), it becomes 0.0.0.0.
pub struct ClientIp(pub IpAddr);

impl axum::extract::FromRequestParts<AppState> for ClientIp {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let peer = parts
            .extensions
            .get::<ConnectInfo<SocketAddr>>()
            .map_or(IpAddr::V4(Ipv4Addr::UNSPECIFIED), |info| info.0.ip());
        let forwarded = || {
            parts
                .headers
                .get("x-forwarded-for")?
                .to_str()
                .ok()?
                .rsplit(',')
                .next()?
                .trim()
                .parse::<IpAddr>()
                .ok()
        };
        let ip = match state.config.trusted_proxy {
            Some(proxy) if proxy == peer => forwarded().unwrap_or(peer),
            _ => peer,
        };
        Ok(Self(group_ipv6(ip)))
    }
}

// PT: Zera os 64 bits finais de um IPv6 (um cliente costuma ter um /64 inteiro).
// EN: Zeroes the last 64 bits of an IPv6 (a client usually holds a whole /64).
fn group_ipv6(ip: IpAddr) -> IpAddr {
    match ip {
        IpAddr::V6(v6) => {
            let mut segments = v6.segments();
            for segment in segments.iter_mut().skip(4) {
                *segment = 0;
            }
            IpAddr::V6(std::net::Ipv6Addr::from(segments))
        }
        v4 => v4,
    }
}

/// PT: Dados públicos do usuário logado. EN: Public data of the logged-in user.
#[derive(Serialize)]
pub struct Me {
    id: Uuid,
    email: String,
    locale: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegisterBody {
    email: String,
    password: String,
    #[serde(default)]
    locale: Option<String>,
    #[serde(default)]
    invite: Option<String>,
}

/// PT: Cria um usuário (com uso de cota e preferências) dentro da transação dada. Usado também
///     pela linha de comando.
/// EN: Creates a user (with quota usage and preferences) inside the given transaction. Also used
///     by the command line.
pub async fn insert_user(
    tx: &mut sqlx::PgConnection,
    id: Uuid,
    email: &str,
    password_hash: &str,
    locale: &str,
) -> ApiResult<()> {
    let inserted = sqlx::query!(
        "INSERT INTO users (id, email, password_hash, locale) VALUES ($1, $2, $3, $4)
         ON CONFLICT DO NOTHING",
        id,
        email,
        password_hash,
        locale
    )
    .execute(&mut *tx)
    .await?;
    if inserted.rows_affected() == 0 {
        return Err(ApiError::Conflict("auth.email_taken"));
    }
    sqlx::query!("INSERT INTO user_usage (user_id) VALUES ($1)", id)
        .execute(&mut *tx)
        .await?;
    sqlx::query!("INSERT INTO user_preferences (user_id) VALUES ($1)", id)
        .execute(&mut *tx)
        .await?;
    Ok(())
}

/// PT: Trava que serializa os cadastros, para o teto de usuários valer mesmo com pedidos paralelos.
/// EN: Lock that serializes registrations, so the user cap holds even with parallel requests.
const REGISTRATION_LOCK: i64 = 0x6266_7265_6769_7374;

/// PT: `POST /auth/register`. O convite (no modo convite) é conferido antes de qualquer hash, e o
///     teto de usuários, o cadastro e o consumo do convite acontecem numa só transação travada.
/// EN: `POST /auth/register`. The invite (in invite mode) is checked before any hashing, and the
///     user cap, the insert and the invite consumption happen in a single locked transaction.
pub async fn register(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    Json(body): Json<RegisterBody>,
) -> ApiResult<StatusCode> {
    state.ensure_writable()?;
    if state.config.registration == Registration::Closed {
        return Err(ApiError::Forbidden("auth.registration_closed"));
    }
    state
        .limiters
        .register
        .check_key(&ip)
        .map_err(|_| ApiError::TooManyRequests)?;
    let email = normalize_email(&body.email)?;
    password::validate(&body.password)?;
    let locale = match body.locale.as_deref() {
        Some("en") => "en",
        _ => "pt-BR",
    };
    let invite_hash = match state.config.registration {
        Registration::Invite => {
            let token = body
                .invite
                .as_deref()
                .ok_or(ApiError::Forbidden("auth.invalid_invite"))?;
            let hash = session::token_hash(token);
            let valid = sqlx::query_scalar!(
                r#"SELECT EXISTS (SELECT 1 FROM invites
                     WHERE token_hash = $1 AND used_by IS NULL AND expires_at > now()) AS "valid!""#,
                hash
            )
            .fetch_one(&state.db)
            .await?;
            if token.len() != 43 || !valid {
                return Err(ApiError::Forbidden("auth.invalid_invite"));
            }
            Some(hash)
        }
        _ => None,
    };

    let password_hash = password::hash(&state, body.password).await?;
    let id = Uuid::now_v7();
    let mut tx = state.db.begin().await?;
    sqlx::query!("SELECT pg_advisory_xact_lock($1)", REGISTRATION_LOCK)
        .execute(&mut *tx)
        .await?;
    let users = sqlx::query_scalar!(r#"SELECT count(*) AS "count!" FROM users"#)
        .fetch_one(&mut *tx)
        .await?;
    if users >= state.config.max_users {
        return Err(ApiError::Forbidden("auth.registration_full"));
    }
    if let Some(invite_hash) = &invite_hash {
        let locked = sqlx::query_scalar!(
            "SELECT token_hash FROM invites
             WHERE token_hash = $1 AND used_by IS NULL AND expires_at > now() FOR UPDATE",
            invite_hash
        )
        .fetch_optional(&mut *tx)
        .await?;
        if locked.is_none() {
            return Err(ApiError::Forbidden("auth.invalid_invite"));
        }
    }
    insert_user(&mut tx, id, &email, &password_hash, locale).await?;
    if let Some(invite_hash) = invite_hash {
        sqlx::query!(
            "UPDATE invites SET used_by = $1, used_at = now() WHERE token_hash = $2",
            id,
            invite_hash
        )
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(StatusCode::CREATED)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LoginBody {
    email: String,
    password: String,
}

/// PT: `POST /auth/login`. Sempre faz um hash (fictício se o e-mail não existe) e devolve a mesma
///     resposta (401) para e-mail inexistente, senha errada ou conta bloqueada, para não revelar
///     quais contas existem. O contador de falhas é atualizado de forma atômica no banco.
/// EN: `POST /auth/login`. Always computes a hash (a dummy one if the email does not exist) and
///     returns the same answer (401) for an unknown email, wrong password or locked account, so it
///     does not reveal which accounts exist. The failure counter is updated atomically in the database.
pub async fn login(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    headers: HeaderMap,
    Json(body): Json<LoginBody>,
) -> ApiResult<Response> {
    state
        .limiters
        .login
        .check_key(&ip)
        .map_err(|_| ApiError::TooManyRequests)?;
    if body.password.chars().count() > password::MAX_PASSWORD {
        return Err(ApiError::Unauthorized);
    }
    let email = normalize_email(&body.email).unwrap_or_default();
    let user = sqlx::query!(
        "SELECT id, password_hash, locked_until > now() AS locked FROM users WHERE lower(email) = $1",
        email
    )
    .fetch_optional(&state.db)
    .await?;

    let Some(user) = user else {
        let _ = password::verify(&state, body.password, state.dummy_hash.to_string()).await;
        return Err(ApiError::Unauthorized);
    };
    let correct = password::verify(&state, body.password, user.password_hash).await?;
    if user.locked == Some(true) {
        return Err(ApiError::Unauthorized);
    }
    if !correct {
        // PT: Incremento atômico: pedidos paralelos somam todos, e o 5º erro bloqueia.
        // EN: Atomic increment: parallel requests all add up, and the 5th failure locks.
        sqlx::query!(
            "UPDATE users SET
                 failed_logins = CASE WHEN failed_logins + 1 >= $2 THEN 0 ELSE failed_logins + 1 END,
                 locked_until = CASE WHEN failed_logins + 1 >= $2 THEN now() + $3 ELSE locked_until END
             WHERE id = $1",
            user.id,
            MAX_FAILED_LOGINS,
            LOCKOUT_INTERVAL
        )
        .execute(&state.db)
        .await?;
        return Err(ApiError::Unauthorized);
    }
    // PT: Confere o bloqueio de novo, de forma atômica, ao zerar o contador (outro pedido pode ter
    //     bloqueado a conta enquanto esta senha era conferida).
    // EN: Checks the lock again, atomically, while resetting the counter (another request may have
    //     locked the account while this password was being verified).
    let unlocked = sqlx::query_scalar!(
        "UPDATE users SET failed_logins = 0, locked_until = NULL
         WHERE id = $1 AND (locked_until IS NULL OR locked_until <= now()) RETURNING id",
        user.id
    )
    .fetch_optional(&state.db)
    .await?;
    if unlocked.is_none() {
        return Err(ApiError::Unauthorized);
    }

    // PT: Sessão sempre nova no login (evita fixação de sessão).
    // EN: Always a new session on login (prevents session fixation).
    let agent = headers
        .get(header::USER_AGENT)
        .and_then(|v| v.to_str().ok());
    let token = session::create(&state, user.id, agent).await?;
    Ok((
        [(header::SET_COOKIE, session::set_cookie(&token)?)],
        StatusCode::NO_CONTENT,
    )
        .into_response())
}

/// PT: `POST /auth/logout`.
pub async fn logout(State(state): State<AppState>, user: CurrentUser) -> ApiResult<Response> {
    session::delete(&state, &user.token_hash).await?;
    Ok((
        [(header::SET_COOKIE, session::clear_cookie())],
        StatusCode::NO_CONTENT,
    )
        .into_response())
}

/// PT: `GET /auth/me`.
pub async fn me(user: CurrentUser) -> Json<Me> {
    Json(Me {
        id: user.id,
        email: user.email,
        locale: user.locale,
    })
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PasswordBody {
    current: String,
    new: String,
}

/// PT: `POST /auth/password`. Confere a senha atual, grava a nova e encerra as outras sessões.
/// EN: `POST /auth/password`. Checks the current password, stores the new one and ends the other sessions.
pub async fn change_password(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(body): Json<PasswordBody>,
) -> ApiResult<StatusCode> {
    state.check_write_rate(user.id)?;
    password::validate(&body.new)?;
    if body.current.chars().count() > password::MAX_PASSWORD {
        return Err(ApiError::Forbidden("auth.wrong_password"));
    }
    let stored = sqlx::query_scalar!("SELECT password_hash FROM users WHERE id = $1", user.id)
        .fetch_one(&state.db)
        .await?;
    if !password::verify(&state, body.current, stored).await? {
        return Err(ApiError::Forbidden("auth.wrong_password"));
    }
    let hash = password::hash(&state, body.new).await?;
    let mut tx = state.db.begin().await?;
    sqlx::query!(
        "UPDATE users SET password_hash = $1, updated_at = now() WHERE id = $2",
        hash,
        user.id
    )
    .execute(&mut *tx)
    .await?;
    sqlx::query!(
        "DELETE FROM sessions WHERE user_id = $1 AND token_hash <> $2",
        user.id,
        user.token_hash
    )
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_and_validates_emails() {
        assert_eq!(
            normalize_email("  User@Example.COM ").unwrap(),
            "user@example.com"
        );
        assert!(normalize_email("no-at-sign").is_err());
        assert!(normalize_email("a@b@c").is_err());
        assert!(normalize_email("@x").is_err());
        assert!(normalize_email("a b@c.d").is_err());
        assert!(normalize_email(&format!("{}@x.y", "a".repeat(260))).is_err());
    }
}
