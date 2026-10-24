//! PT: Servidor do blueprint-forge: serve a interface, os pacotes do jogo e a API com login.
//!     Escuta só em 127.0.0.1 por padrão. Subcomandos: `serve` (padrão), `create-user` e
//!     `create-invite`.
//! EN: blueprint-forge server: serves the interface, the game packages and the API with login.
//!     It listens only on 127.0.0.1 by default. Subcommands: `serve` (default), `create-user` and
//!     `create-invite`.

mod analysis;
mod app;
mod auth;
mod config;
mod error;
mod game;
mod maintenance;
mod routes;
mod security;
mod state;

use std::io::BufRead;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use sqlx::postgres::PgPoolOptions;
use tracing_subscriber::EnvFilter;
use uuid::Uuid;

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();
    let runtime = tokio::runtime::Runtime::new()?;
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        None | Some("serve") => runtime.block_on(serve()),
        Some("create-user") => {
            let email = args
                .get(1)
                .context("uso / usage: bf-server create-user <email>")?;
            runtime.block_on(create_user(email))
        }
        Some("create-invite") => runtime.block_on(create_invite()),
        Some(other) => bail!("subcomando desconhecido / unknown subcommand: {other}"),
    }
}

async fn connect(config: &config::Config) -> Result<sqlx::PgPool> {
    PgPoolOptions::new()
        .max_connections(10)
        .acquire_timeout(Duration::from_secs(5))
        .connect(&config.database_url)
        .await
        .context("falha ao conectar no banco / failed to connect to the database")
}

async fn serve() -> Result<()> {
    let config = config::Config::from_env()?;
    let store = Arc::new(game::GameStore::load(&config.game_assets_dir)?);
    let db = connect(&config).await?;
    let dummy =
        auth::password::hash_now("dummy password for timing").map_err(|e| anyhow!("{e}"))?;
    let bind = config.bind;
    let web_dist = config.web_dist.clone();
    let state = state::AppState::new(db, store.clone(), config, dummy);
    tokio::spawn(maintenance::run_forever(state.clone()));
    let router = app::router(store, Some(state), &web_dist);

    let listener = tokio::net::TcpListener::bind(bind).await?;
    tracing::info!(address = %bind, "servidor ouvindo / server listening");
    axum::serve(
        listener,
        router.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(async {
        // PT: Se o sinal falhar, encerra mesmo assim. EN: If the signal fails, shut down anyway.
        let _ = tokio::signal::ctrl_c().await;
    })
    .await?;
    Ok(())
}

// PT: Cria um usuário pela linha de comando; a senha vem da entrada padrão (uma linha).
// EN: Creates a user from the command line; the password comes from standard input (one line).
async fn create_user(email: &str) -> Result<()> {
    let config = config::Config::from_env()?;
    let email = auth::routes::normalize_email(email).map_err(|e| anyhow!("{e}"))?;
    eprintln!("senha (uma linha na entrada padrão) / password (one line on standard input):");
    let mut password = String::new();
    std::io::stdin().lock().read_line(&mut password)?;
    let password = password.trim_end_matches(['\r', '\n']).to_owned();
    auth::password::validate(&password).map_err(|e| anyhow!("{e}"))?;
    let hash = auth::password::hash_now(&password).map_err(|e| anyhow!("{e}"))?;
    let db = connect(&config).await?;
    let mut tx = db.begin().await?;
    auth::routes::insert_user(&mut tx, Uuid::now_v7(), &email, &hash, "pt-BR")
        .await
        .map_err(|e| anyhow!("{e}"))?;
    tx.commit().await?;
    println!("usuário criado / user created: {email}");
    Ok(())
}

// PT: Cria um convite de uso único válido por 7 dias e mostra o token (só agora; o banco guarda o hash).
// EN: Creates a single-use invite valid for 7 days and shows the token (only now; the database keeps the hash).
async fn create_invite() -> Result<()> {
    let config = config::Config::from_env()?;
    let db = connect(&config).await?;
    let (token, hash) = auth::session::new_token().map_err(|e| anyhow!("{e}"))?;
    sqlx::query!(
        "INSERT INTO invites (token_hash, expires_at) VALUES ($1, now() + interval '7 days')",
        hash
    )
    .execute(&db)
    .await?;
    println!("{token}");
    Ok(())
}

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_db;
