//! PT: `POST /analyze`: análise sem salvar, para quando o navegador não roda WASM e para testes.
//!     Exige login, para que ninguém anônimo use a CPU do servidor.
//! EN: `POST /analyze`: analysis without saving, for when the browser cannot run WASM and for tests.
//!     It requires login, so nobody anonymous can use the server's CPU.

use axum::Json;
use axum::extract::State;
use bf_calc::AnalysisOptions;
use serde::Deserialize;

use crate::analysis;
use crate::auth::CurrentUser;
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Body {
    source: String,
    #[serde(default)]
    selected: usize,
    #[serde(default)]
    options: AnalysisOptions,
}

/// PT: `POST /analyze`.
pub async fn analyze(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(body): Json<Body>,
) -> ApiResult<Json<bf_analysis::Analysis>> {
    state.check_write_rate(user.id)?;
    body.options.validate().map_err(ApiError::BadRequest)?;
    let result = analysis::run(&state, user.id, body.source, body.selected, body.options).await?;
    Ok(Json(result.analysis))
}
