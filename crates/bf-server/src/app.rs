//! PT: Montagem do roteador com as camadas de segurança: limite de corpo, tempo limite,
//!     cabeçalhos (CSP e outros), checagem de origem e log das requisições sem corpo.
//! EN: Router assembly with the security layers: body limit, timeout, headers (CSP and others),
//!     origin check and request logging without bodies.

use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::extract::DefaultBodyLimit;
use axum::http::{HeaderName, HeaderValue, StatusCode, header};
use axum::middleware;
use axum::routing::{get, post};
use tower_http::compression::CompressionLayer;
use tower_http::services::{ServeDir, ServeFile};
use tower_http::set_header::SetResponseHeaderLayer;
use tower_http::timeout::TimeoutLayer;
use tower_http::trace::TraceLayer;

use crate::auth::routes as auth;
use crate::error::ApiError;
use crate::game::{self, GameStore};
use crate::routes::{analyze, blueprints, preferences, projects};
use crate::security::check_origin;
use crate::state::AppState;

/// PT: Tamanho máximo do corpo de qualquer requisição.
/// EN: Maximum body size of any request.
pub const MAX_BODY_BYTES: usize = 10 * 1024 * 1024;
/// PT: Tempo máximo de uma requisição.
/// EN: Maximum time for one request.
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// PT: Política de conteúdo: só recursos da própria origem, WASM permitido, sem scripts inline.
/// EN: Content policy: same-origin resources only, WASM allowed, no inline scripts.
pub const CONTENT_SECURITY_POLICY: &str = "default-src 'self'; script-src 'self' 'wasm-unsafe-eval'; \
style-src 'self'; img-src 'self' data: blob:; connect-src 'self'; worker-src 'self'; \
object-src 'none'; base-uri 'none'; form-action 'self'; frame-ancestors 'none'";

/// PT: Rotas públicas do pacote do jogo (não precisam de banco).
/// EN: Public game package routes (they do not need the database).
pub fn game_routes(store: Arc<GameStore>) -> Router {
    Router::new()
        .route("/health", get(|| async { "ok" }))
        .route("/game/versions", get(game::versions))
        .route("/game/{version}/data", get(game::data))
        .route("/game/{version}/locale/{lang}", get(game::locale))
        .route("/assets/icons/{version}/{file}", get(game::icon))
        .with_state(store)
}

/// PT: Rotas de conta e dados do usuário.
/// EN: Account and user data routes.
pub fn user_routes(state: AppState) -> Router {
    Router::new()
        .route("/auth/register", post(auth::register))
        .route("/auth/login", post(auth::login))
        .route("/auth/logout", post(auth::logout))
        .route("/auth/me", get(auth::me))
        .route("/auth/password", post(auth::change_password))
        .route("/preferences", get(preferences::get).put(preferences::put))
        .route("/analyze", post(analyze::analyze))
        .route(
            "/blueprints",
            get(blueprints::list).post(blueprints::create),
        )
        .route(
            "/blueprints/{id}",
            get(blueprints::get)
                .patch(blueprints::update)
                .delete(blueprints::delete),
        )
        .route("/blueprints/{id}/analyses", post(blueprints::analyze))
        .route("/projects", get(projects::list).post(projects::create))
        .route(
            "/projects/{id}",
            get(projects::get)
                .patch(projects::update)
                .delete(projects::delete),
        )
        .route("/projects/{id}/blueprints", post(projects::add_blueprint))
        .route(
            "/projects/{id}/blueprints/{blueprint_id}",
            axum::routing::delete(projects::remove_blueprint),
        )
        .layer(middleware::from_fn_with_state(state.clone(), check_origin))
        .with_state(state)
}

/// PT: Monta o roteador completo. EN: Builds the complete router.
pub fn router(
    store: Arc<GameStore>,
    state: Option<AppState>,
    web_dist: &std::path::Path,
) -> Router {
    let mut api = game_routes(store);
    if let Some(state) = state {
        api = api.merge(user_routes(state));
    }
    let api = api.fallback(|| async { ApiError::NotFound });

    // PT: A interface é uma página única: caminhos desconhecidos caem no index.html.
    // EN: The interface is a single page: unknown paths fall back to index.html.
    let spa = ServeDir::new(web_dist).fallback(ServeFile::new(web_dist.join("index.html")));

    Router::new()
        .nest("/api/v1", api)
        .fallback_service(spa)
        .layer(DefaultBodyLimit::max(MAX_BODY_BYTES))
        .layer(CompressionLayer::new())
        .layer(TimeoutLayer::with_status_code(
            StatusCode::REQUEST_TIMEOUT,
            REQUEST_TIMEOUT,
        ))
        .layer(security_header(
            header::CONTENT_SECURITY_POLICY,
            CONTENT_SECURITY_POLICY,
        ))
        .layer(security_header(header::X_CONTENT_TYPE_OPTIONS, "nosniff"))
        .layer(security_header(header::REFERRER_POLICY, "no-referrer"))
        .layer(security_header(header::X_FRAME_OPTIONS, "DENY"))
        .layer(security_header(
            HeaderName::from_static("cross-origin-opener-policy"),
            "same-origin",
        ))
        .layer(security_header(
            HeaderName::from_static("permissions-policy"),
            "camera=(), microphone=(), geolocation=(), payment=()",
        ))
        .layer(TraceLayer::new_for_http())
}

fn security_header(name: HeaderName, value: &'static str) -> SetResponseHeaderLayer<HeaderValue> {
    SetResponseHeaderLayer::overriding(name, HeaderValue::from_static(value))
}
