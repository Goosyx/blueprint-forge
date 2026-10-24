//! PT: Preferências do usuário (unidade de taxa e padrões da análise e do gerador).
//! EN: User preferences (rate unit and analysis and generator defaults).

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};

use crate::auth::CurrentUser;
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;

/// PT: Tamanho máximo de cada objeto de padrões. EN: Maximum size of each defaults object.
const MAX_DEFAULTS_BYTES: usize = 8 * 1024;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Preferences {
    rate_unit: String,
    analysis_defaults: serde_json::Value,
    generator_defaults: serde_json::Value,
}

/// PT: `GET /preferences`.
pub async fn get(State(state): State<AppState>, user: CurrentUser) -> ApiResult<Json<Preferences>> {
    let row = sqlx::query_as!(
        Preferences,
        "SELECT rate_unit, analysis_defaults, generator_defaults FROM user_preferences WHERE user_id = $1",
        user.id
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or(ApiError::NotFound)?;
    Ok(Json(row))
}

/// PT: `PUT /preferences`.
pub async fn put(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(body): Json<Preferences>,
) -> ApiResult<StatusCode> {
    state.ensure_writable()?;
    state.check_write_rate(user.id)?;
    if !matches!(body.rate_unit.as_str(), "second" | "minute" | "hour") {
        return Err(ApiError::BadRequest("preferences.invalid_unit"));
    }
    for value in [&body.analysis_defaults, &body.generator_defaults] {
        if !value.is_object() || value.to_string().len() > MAX_DEFAULTS_BYTES {
            return Err(ApiError::BadRequest("preferences.invalid_defaults"));
        }
    }
    sqlx::query!(
        "UPDATE user_preferences SET rate_unit = $2, analysis_defaults = $3, generator_defaults = $4,
                                    updated_at = now()
         WHERE user_id = $1",
        user.id,
        body.rate_unit,
        body.analysis_defaults,
        body.generator_defaults
    )
    .execute(&state.db)
    .await?;
    Ok(StatusCode::NO_CONTENT)
}
