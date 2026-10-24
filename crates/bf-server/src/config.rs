//! PT: Configuração do servidor lida das variáveis de ambiente, com padrões seguros para uso local.
//! EN: Server configuration read from environment variables, with safe defaults for local use.

use std::net::SocketAddr;
use std::path::PathBuf;

use anyhow::{Context, Result, bail};

/// PT: Modo de cadastro de contas.
/// EN: Account registration mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Registration {
    /// PT: Só pela linha de comando (`bf-server create-user`). EN: Only via the command line (`bf-server create-user`).
    Closed,
    /// PT: Só com convite de uso único. EN: Only with a single-use invite.
    Invite,
    /// PT: Aberto, com teto de usuários e limite por IP. EN: Open, with a user cap and a per-IP limit.
    Open,
}

/// PT: Cotas por usuário. EN: Per-user quotas.
#[derive(Debug, Clone)]
pub struct Quotas {
    pub blueprints: i32,
    pub bytes: i64,
    pub projects: i32,
    pub blueprints_per_project: i64,
    pub sessions: i64,
    pub analyses_per_blueprint: i64,
}

impl Default for Quotas {
    fn default() -> Self {
        Self {
            blueprints: 500,
            bytes: 50 * 1024 * 1024,
            projects: 100,
            blueprints_per_project: 200,
            sessions: 10,
            analyses_per_blueprint: 5,
        }
    }
}

/// PT: Configuração do servidor.
/// EN: Server configuration.
#[derive(Debug, Clone)]
pub struct Config {
    /// PT: Endereço de escuta; o padrão só aceita conexões locais.
    /// EN: Listen address; the default only accepts local connections.
    pub bind: SocketAddr,
    pub database_url: String,
    pub game_assets_dir: PathBuf,
    pub web_dist: PathBuf,
    /// PT: Origem aceita nas requisições que mudam estado (proteção contra CSRF).
    /// EN: Origin accepted on state-changing requests (CSRF protection).
    pub allowed_origin: String,
    pub registration: Registration,
    pub max_users: i64,
    pub max_db_bytes: i64,
    pub quotas: Quotas,
    /// PT: IP do proxy reverso confiável; só dele o `X-Forwarded-For` é aceito.
    /// EN: Trusted reverse proxy IP; `X-Forwarded-For` is only accepted from it.
    pub trusted_proxy: Option<std::net::IpAddr>,
}

fn var(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|value| !value.is_empty())
}

impl Config {
    /// PT: Lê a configuração das variáveis `BF_*` e `DATABASE_URL`.
    /// EN: Reads the configuration from the `BF_*` and `DATABASE_URL` variables.
    pub fn from_env() -> Result<Self> {
        let bind = var("BF_BIND").unwrap_or_else(|| "127.0.0.1:8080".to_owned());
        let registration = match var("BF_REGISTRATION").as_deref().unwrap_or("closed") {
            "closed" => Registration::Closed,
            "invite" => Registration::Invite,
            "open" => Registration::Open,
            other => bail!("BF_REGISTRATION inválido / invalid BF_REGISTRATION: {other:?}"),
        };
        let number = |name: &str, default: i64| -> Result<i64> {
            match var(name) {
                Some(value) => value
                    .parse()
                    .with_context(|| format!("{name} inválido / invalid {name}")),
                None => Ok(default),
            }
        };
        Ok(Self {
            bind: bind
                .parse()
                .with_context(|| format!("BF_BIND inválido / invalid BF_BIND: {bind:?}"))?,
            database_url: var("DATABASE_URL")
                .context("DATABASE_URL ausente / missing DATABASE_URL")?,
            game_assets_dir: var("BF_GAME_ASSETS_DIR")
                .map_or_else(|| PathBuf::from("game-assets"), PathBuf::from),
            web_dist: var("BF_WEB_DIST").map_or_else(|| PathBuf::from("web/dist"), PathBuf::from),
            allowed_origin: var("BF_ALLOWED_ORIGIN")
                .unwrap_or_else(|| "http://localhost:8080".to_owned()),
            registration,
            max_users: number("BF_MAX_USERS", 50)?,
            max_db_bytes: number("BF_MAX_DB_BYTES", 2 * 1024 * 1024 * 1024)?,
            quotas: Quotas::default(),
            trusted_proxy: var("BF_TRUSTED_PROXY")
                .map(|value| {
                    value
                        .parse()
                        .context("BF_TRUSTED_PROXY inválido / invalid BF_TRUSTED_PROXY")
                })
                .transpose()?,
        })
    }
}
