//! PT: Estado compartilhado do servidor: banco, pacotes do jogo, configuração, semáforos que
//!     limitam o trabalho pesado (análise e hash de senha) e limitadores de taxa.
//! EN: Shared server state: database, game packages, configuration, semaphores that limit heavy
//!     work (analysis and password hashing) and rate limiters.

use std::collections::HashMap;
use std::net::IpAddr;
use std::num::NonZeroU32;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use governor::{DefaultKeyedRateLimiter, Quota, RateLimiter};
use sqlx::PgPool;
use tokio::sync::Semaphore;
use uuid::Uuid;

use crate::config::Config;
use crate::error::ApiError;
use crate::game::GameStore;

/// PT: Limitadores de taxa por IP (login, cadastro) e por usuário (gravações).
/// EN: Rate limiters per IP (login, registration) and per user (writes).
pub struct Limiters {
    pub login: DefaultKeyedRateLimiter<IpAddr>,
    pub register: DefaultKeyedRateLimiter<IpAddr>,
    pub writes: DefaultKeyedRateLimiter<Uuid>,
}

impl Limiters {
    fn new() -> Self {
        let per_minute = |count: u32, burst: u32| {
            Quota::per_minute(NonZeroU32::new(count).unwrap_or(NonZeroU32::MIN))
                .allow_burst(NonZeroU32::new(burst).unwrap_or(NonZeroU32::MIN))
        };
        Self {
            login: RateLimiter::keyed(per_minute(10, 5)),
            register: RateLimiter::keyed(per_minute(3, 3)),
            writes: RateLimiter::keyed(per_minute(30, 10)),
        }
    }
}

/// PT: Estado do servidor, clonado barato entre as rotas.
/// EN: Server state, cheaply cloned across routes.
#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub store: Arc<GameStore>,
    pub config: Arc<Config>,
    /// PT: Análises simultâneas no servidor (uma por CPU). EN: Simultaneous analyses on the server (one per CPU).
    pub analysis_slots: Arc<Semaphore>,
    /// PT: Hashes de senha simultâneos (Argon2 usa ~19 MB cada). EN: Simultaneous password hashes (Argon2 uses ~19 MB each).
    pub hash_slots: Arc<Semaphore>,
    /// PT: Análises em andamento por usuário. EN: Analyses in progress per user.
    pub running: Arc<Mutex<HashMap<Uuid, u32>>>,
    pub limiters: Arc<Limiters>,
    /// PT: Modo somente leitura quando o banco passa do teto global. EN: Read-only mode when the database exceeds the global cap.
    pub read_only: Arc<AtomicBool>,
    /// PT: Hash fictício usado quando o e-mail não existe (tempo constante no login).
    /// EN: Dummy hash used when the email does not exist (constant time on login).
    pub dummy_hash: Arc<String>,
}

/// PT: Análises simultâneas permitidas por usuário. EN: Simultaneous analyses allowed per user.
pub const ANALYSES_PER_USER: u32 = 2;

impl AppState {
    /// PT: Monta o estado. EN: Builds the state.
    pub fn new(db: PgPool, store: Arc<GameStore>, config: Config, dummy_hash: String) -> Self {
        let cpus = std::thread::available_parallelism().map_or(2, std::num::NonZero::get);
        Self {
            db,
            store,
            config: Arc::new(config),
            analysis_slots: Arc::new(Semaphore::new(cpus)),
            hash_slots: Arc::new(Semaphore::new(2)),
            running: Arc::new(Mutex::new(HashMap::new())),
            limiters: Arc::new(Limiters::new()),
            read_only: Arc::new(AtomicBool::new(false)),
            dummy_hash: Arc::new(dummy_hash),
        }
    }

    /// PT: Recusa gravações no modo somente leitura. EN: Refuses writes in read-only mode.
    pub fn ensure_writable(&self) -> Result<(), ApiError> {
        if self.read_only.load(Ordering::Relaxed) {
            return Err(ApiError::Unavailable("storage.full"));
        }
        Ok(())
    }

    /// PT: Aplica o limite de gravações por usuário. EN: Applies the per-user write limit.
    pub fn check_write_rate(&self, user: Uuid) -> Result<(), ApiError> {
        self.limiters
            .writes
            .check_key(&user)
            .map_err(|_| ApiError::TooManyRequests)
    }
}

/// PT: Vaga de análise de um usuário; libera ao sair de escopo.
/// EN: A user's analysis slot; released when it goes out of scope.
pub struct UserSlot {
    running: Arc<Mutex<HashMap<Uuid, u32>>>,
    user: Uuid,
}

impl UserSlot {
    /// PT: Reserva uma vaga se o usuário tiver menos de `ANALYSES_PER_USER` em andamento.
    /// EN: Reserves a slot if the user has fewer than `ANALYSES_PER_USER` in progress.
    pub fn acquire(state: &AppState, user: Uuid) -> Result<Self, ApiError> {
        let mut running = state.running.lock().map_err(|_| ApiError::Internal)?;
        let count = running.entry(user).or_insert(0);
        if *count >= ANALYSES_PER_USER {
            return Err(ApiError::TooManyRequests);
        }
        *count += 1;
        Ok(Self {
            running: state.running.clone(),
            user,
        })
    }
}

impl Drop for UserSlot {
    fn drop(&mut self) {
        if let Ok(mut running) = self.running.lock()
            && let Some(count) = running.get_mut(&self.user)
        {
            *count = count.saturating_sub(1);
            if *count == 0 {
                running.remove(&self.user);
            }
        }
    }
}
