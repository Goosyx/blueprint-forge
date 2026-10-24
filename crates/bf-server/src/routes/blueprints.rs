//! PT: Blueprints salvas: o servidor decodifica e analisa de novo antes de gravar, deduplica pelo
//!     sha256 do texto e atualiza a cota na mesma transação (sem corrida entre pedidos paralelos).
//! EN: Saved blueprints: the server decodes and analyzes again before storing, deduplicates by the
//!     text's sha256 and updates the quota in the same transaction (no race between parallel requests).

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use bf_blueprint::Limits;
use bf_calc::AnalysisOptions;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use time::OffsetDateTime;
use uuid::Uuid;

use super::{Page, clean_text};
use crate::analysis::{self, ANALYZER_VERSION};
use crate::auth::CurrentUser;
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;

/// PT: Item da lista. EN: List item.
#[derive(Serialize)]
pub struct BlueprintItem {
    id: Uuid,
    title: String,
    description: Option<String>,
    is_book: bool,
    blueprint_count: i32,
    entity_count: i32,
    #[serde(with = "time::serde::rfc3339")]
    created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    updated_at: OffsetDateTime,
}

/// PT: Análise salva. EN: Saved analysis.
#[derive(Serialize)]
pub struct SavedAnalysis {
    id: Uuid,
    selected: i32,
    options: serde_json::Value,
    summary: serde_json::Value,
    data_version: String,
    #[serde(with = "time::serde::rfc3339")]
    created_at: OffsetDateTime,
}

/// PT: Blueprint com o texto e as análises. EN: Blueprint with its text and analyses.
#[derive(Serialize)]
pub struct BlueprintDetail {
    #[serde(flatten)]
    item: BlueprintItem,
    source: String,
    analyses: Vec<SavedAnalysis>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateBody {
    source: String,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    description: Option<String>,
}

/// PT: `GET /blueprints`.
pub async fn list(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(page): Query<Page>,
) -> ApiResult<Json<Vec<BlueprintItem>>> {
    let rows = sqlx::query_as!(
        BlueprintItem,
        "SELECT id, title, description, is_book, blueprint_count, entity_count, created_at, updated_at
         FROM blueprints WHERE user_id = $1 AND ($2::uuid IS NULL OR id < $2)
         ORDER BY id DESC LIMIT $3",
        user.id,
        page.after,
        page.limit()
    )
    .fetch_all(&state.db)
    .await?;
    Ok(Json(rows))
}

/// PT: `POST /blueprints`. Devolve 201 com a blueprint nova, ou 200 com a existente (mesmo texto).
/// EN: `POST /blueprints`. Returns 201 with the new blueprint, or 200 with the existing one (same text).
pub async fn create(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(body): Json<CreateBody>,
) -> ApiResult<(StatusCode, Json<BlueprintItem>)> {
    state.ensure_writable()?;
    state.check_write_rate(user.id)?;
    let source = body.source.trim().to_owned();
    if source.len() > Limits::default().max_encoded_len {
        return Err(ApiError::BadRequest("blueprint.too_large"));
    }
    let description = clean_text(
        body.description.as_deref(),
        2000,
        "blueprint.description_invalid",
    )?;
    let requested_title = clean_text(body.title.as_deref(), 200, "blueprint.title_invalid")?;
    let sha = Sha256::digest(source.as_bytes()).to_vec();

    if let Some(existing) = find_by_sha(&state, user.id, &sha).await? {
        return Ok((StatusCode::OK, Json(existing)));
    }

    let result = analysis::run(
        &state,
        user.id,
        source.clone(),
        0,
        AnalysisOptions::default(),
    )
    .await?;
    let summary = &result.summary;
    // PT: O nome da blueprint passa pela mesma limpeza do título digitado; vazio vira "Blueprint".
    // EN: The blueprint's label goes through the same cleaning as a typed title; empty becomes "Blueprint".
    let label: Option<String> = summary
        .label
        .as_deref()
        .map(|l| l.chars().filter(|c| !c.is_control()).collect::<String>());
    let title = requested_title
        .or_else(|| {
            clean_text(label.as_deref(), 200, "blueprint.title_invalid")
                .ok()
                .flatten()
        })
        .unwrap_or_else(|| "Blueprint".to_owned());
    let bytes = i64::try_from(source.len()).map_err(|_| ApiError::Internal)?;
    let count = |n: usize| i32::try_from(n).map_err(|_| ApiError::Internal);
    let quotas = &state.config.quotas;
    let id = Uuid::now_v7();

    let mut tx = state.db.begin().await?;
    let reserved = sqlx::query_scalar!(
        "UPDATE user_usage SET blueprints = blueprints + 1, bytes = bytes + $2, updated_at = now()
         WHERE user_id = $1 AND blueprints + 1 <= $3 AND bytes + $2 <= $4
         RETURNING user_id",
        user.id,
        bytes,
        quotas.blueprints,
        quotas.bytes
    )
    .fetch_optional(&mut *tx)
    .await?;
    if reserved.is_none() {
        return Err(ApiError::Conflict("quota.exceeded"));
    }
    let inserted = sqlx::query!(
        "INSERT INTO blueprints (id, user_id, title, description, source, content_sha256, is_book,
                                 blueprint_count, entity_count)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
         ON CONFLICT (user_id, content_sha256) DO NOTHING",
        id,
        user.id,
        title,
        description,
        source,
        sha,
        summary.is_book,
        count(summary.blueprint_count)?,
        count(summary.entity_count)?
    )
    .execute(&mut *tx)
    .await?;
    if inserted.rows_affected() == 0 {
        // PT: Outro pedido gravou o mesmo texto ao mesmo tempo: desfaz a cota e devolve o existente.
        // EN: Another request stored the same text at the same time: undo the quota and return the existing one.
        tx.rollback().await?;
        let existing = find_by_sha(&state, user.id, &sha)
            .await?
            .ok_or(ApiError::Internal)?;
        return Ok((StatusCode::OK, Json(existing)));
    }
    save_analysis(
        &mut tx,
        &state,
        id,
        user.id,
        0,
        &AnalysisOptions::default(),
        &result,
    )
    .await?;
    tx.commit().await?;

    let item = find_by_sha(&state, user.id, &sha)
        .await?
        .ok_or(ApiError::Internal)?;
    Ok((StatusCode::CREATED, Json(item)))
}

async fn find_by_sha(state: &AppState, user: Uuid, sha: &[u8]) -> ApiResult<Option<BlueprintItem>> {
    Ok(sqlx::query_as!(
        BlueprintItem,
        "SELECT id, title, description, is_book, blueprint_count, entity_count, created_at, updated_at
         FROM blueprints WHERE user_id = $1 AND content_sha256 = $2",
        user,
        sha
    )
    .fetch_optional(&state.db)
    .await?)
}

// PT: Grava (ou substitui) a análise das mesmas opções e mantém só as mais recentes por blueprint.
// EN: Stores (or replaces) the analysis for the same options and keeps only the newest per blueprint.
async fn save_analysis(
    tx: &mut sqlx::PgConnection,
    state: &AppState,
    blueprint: Uuid,
    user: Uuid,
    selected: i32,
    options: &AnalysisOptions,
    result: &analysis::ServerAnalysis,
) -> ApiResult<Uuid> {
    let options_json = serde_json::to_value(options).map_err(|_| ApiError::Internal)?;
    let options_sha = Sha256::digest(options_json.to_string().as_bytes()).to_vec();
    let summary = serde_json::to_value(&result.summary).map_err(|_| ApiError::Internal)?;
    let id = sqlx::query_scalar!(
        "INSERT INTO blueprint_analyses (id, blueprint_id, user_id, selected, options_sha256, options,
                                         summary, data_version, analyzer_version)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
         ON CONFLICT (blueprint_id, selected, options_sha256)
         DO UPDATE SET summary = EXCLUDED.summary, data_version = EXCLUDED.data_version,
                       analyzer_version = EXCLUDED.analyzer_version, created_at = now()
         RETURNING id",
        Uuid::now_v7(),
        blueprint,
        user,
        selected,
        options_sha,
        options_json,
        summary,
        result.data_version,
        ANALYZER_VERSION
    )
    .fetch_one(&mut *tx)
    .await?;
    sqlx::query!(
        "DELETE FROM blueprint_analyses WHERE blueprint_id = $1 AND id IN (
             SELECT id FROM blueprint_analyses WHERE blueprint_id = $1
             ORDER BY created_at DESC OFFSET $2)",
        blueprint,
        state.config.quotas.analyses_per_blueprint
    )
    .execute(&mut *tx)
    .await?;
    Ok(id)
}

/// PT: `GET /blueprints/{id}`.
pub async fn get(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<BlueprintDetail>> {
    let row = sqlx::query!(
        "SELECT id, title, description, is_book, blueprint_count, entity_count, created_at, updated_at, source
         FROM blueprints WHERE id = $1 AND user_id = $2",
        id,
        user.id
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or(ApiError::NotFound)?;
    let analyses = sqlx::query_as!(
        SavedAnalysis,
        "SELECT id, selected, options, summary, data_version, created_at
         FROM blueprint_analyses WHERE blueprint_id = $1 AND user_id = $2 ORDER BY created_at DESC",
        id,
        user.id
    )
    .fetch_all(&state.db)
    .await?;
    Ok(Json(BlueprintDetail {
        item: BlueprintItem {
            id: row.id,
            title: row.title,
            description: row.description,
            is_book: row.is_book,
            blueprint_count: row.blueprint_count,
            entity_count: row.entity_count,
            created_at: row.created_at,
            updated_at: row.updated_at,
        },
        source: row.source,
        analyses,
    }))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateBody {
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    description: Option<String>,
}

/// PT: `PATCH /blueprints/{id}`: troca título e descrição.
/// EN: `PATCH /blueprints/{id}`: changes title and description.
pub async fn update(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateBody>,
) -> ApiResult<StatusCode> {
    state.ensure_writable()?;
    state.check_write_rate(user.id)?;
    let title = clean_text(body.title.as_deref(), 200, "blueprint.title_invalid")?;
    let description = clean_text(
        body.description.as_deref(),
        2000,
        "blueprint.description_invalid",
    )?;
    let updated = sqlx::query!(
        "UPDATE blueprints SET title = COALESCE($3, title),
                               description = CASE WHEN $4::boolean THEN $5 ELSE description END,
                               updated_at = now()
         WHERE id = $1 AND user_id = $2",
        id,
        user.id,
        title,
        body.description.is_some(),
        description
    )
    .execute(&state.db)
    .await?;
    if updated.rows_affected() == 0 {
        return Err(ApiError::NotFound);
    }
    Ok(StatusCode::NO_CONTENT)
}

/// PT: `DELETE /blueprints/{id}`: apaga e devolve a cota na mesma transação.
/// EN: `DELETE /blueprints/{id}`: deletes and returns the quota in the same transaction.
pub async fn delete(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<StatusCode> {
    state.check_write_rate(user.id)?;
    let mut tx = state.db.begin().await?;
    let bytes = sqlx::query_scalar!(
        r#"DELETE FROM blueprints WHERE id = $1 AND user_id = $2 RETURNING octet_length(source)::bigint AS "bytes!""#,
        id,
        user.id
    )
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(ApiError::NotFound)?;
    sqlx::query!(
        "UPDATE user_usage SET blueprints = GREATEST(blueprints - 1, 0),
                               bytes = GREATEST(bytes - $2, 0), updated_at = now()
         WHERE user_id = $1",
        user.id,
        bytes
    )
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnalyzeBody {
    #[serde(default)]
    selected: usize,
    #[serde(default)]
    options: AnalysisOptions,
}

/// PT: `POST /blueprints/{id}/analyses`: recalcula com novas opções e guarda o resumo.
/// EN: `POST /blueprints/{id}/analyses`: recalculates with new options and stores the summary.
pub async fn analyze(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    Json(body): Json<AnalyzeBody>,
) -> ApiResult<Json<bf_analysis::Analysis>> {
    state.ensure_writable()?;
    state.check_write_rate(user.id)?;
    body.options.validate().map_err(ApiError::BadRequest)?;
    let selected = i32::try_from(body.selected)
        .map_err(|_| ApiError::BadRequest("analysis.no_such_blueprint"))?;
    let source = sqlx::query_scalar!(
        "SELECT source FROM blueprints WHERE id = $1 AND user_id = $2",
        id,
        user.id
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or(ApiError::NotFound)?;
    let result =
        analysis::run(&state, user.id, source, body.selected, body.options.clone()).await?;
    let mut tx = state.db.begin().await?;
    save_analysis(
        &mut tx,
        &state,
        id,
        user.id,
        selected,
        &body.options,
        &result,
    )
    .await?;
    tx.commit().await?;
    Ok(Json(result.analysis))
}
