//! PT: Projetos de fábrica: agrupam blueprints do próprio usuário. A cota de projetos é atualizada na
//!     mesma transação, e as chaves compostas do banco impedem misturar dados de usuários diferentes.
//! EN: Factory projects: they group the user's own blueprints. The project quota is updated in the
//!     same transaction, and the database's composite keys prevent mixing different users' data.

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

use super::{Page, clean_text};
use crate::auth::CurrentUser;
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;

#[derive(Serialize)]
pub struct Project {
    id: Uuid,
    name: String,
    description: Option<String>,
    blueprints: i64,
    #[serde(with = "time::serde::rfc3339")]
    created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    updated_at: OffsetDateTime,
}

#[derive(Serialize)]
pub struct ProjectBlueprint {
    id: Uuid,
    title: String,
    position: i32,
    note: Option<String>,
}

#[derive(Serialize)]
pub struct ProjectDetail {
    #[serde(flatten)]
    project: Project,
    items: Vec<ProjectBlueprint>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectBody {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    description: Option<String>,
}

/// PT: `GET /projects`.
pub async fn list(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(page): Query<Page>,
) -> ApiResult<Json<Vec<Project>>> {
    let rows = sqlx::query_as!(
        Project,
        r#"SELECT p.id, p.name, p.description, p.created_at, p.updated_at,
                  (SELECT count(*) FROM project_blueprints pb WHERE pb.project_id = p.id) AS "blueprints!"
           FROM projects p WHERE p.user_id = $1 AND ($2::uuid IS NULL OR p.id < $2)
           ORDER BY p.id DESC LIMIT $3"#,
        user.id,
        page.after,
        page.limit()
    )
    .fetch_all(&state.db)
    .await?;
    Ok(Json(rows))
}

/// PT: `POST /projects`.
pub async fn create(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(body): Json<ProjectBody>,
) -> ApiResult<(StatusCode, Json<serde_json::Value>)> {
    state.ensure_writable()?;
    state.check_write_rate(user.id)?;
    let name = clean_text(body.name.as_deref(), 120, "project.name_invalid")?
        .ok_or(ApiError::BadRequest("project.name_invalid"))?;
    let description = clean_text(
        body.description.as_deref(),
        2000,
        "project.description_invalid",
    )?;
    let id = Uuid::now_v7();
    let mut tx = state.db.begin().await?;
    let reserved = sqlx::query_scalar!(
        "UPDATE user_usage SET projects = projects + 1, updated_at = now()
         WHERE user_id = $1 AND projects + 1 <= $2 RETURNING user_id",
        user.id,
        state.config.quotas.projects
    )
    .fetch_optional(&mut *tx)
    .await?;
    if reserved.is_none() {
        return Err(ApiError::Conflict("quota.exceeded"));
    }
    let inserted = sqlx::query!(
        "INSERT INTO projects (id, user_id, name, description) VALUES ($1, $2, $3, $4) ON CONFLICT DO NOTHING",
        id,
        user.id,
        name,
        description
    )
    .execute(&mut *tx)
    .await?;
    if inserted.rows_affected() == 0 {
        return Err(ApiError::Conflict("project.name_taken"));
    }
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(serde_json::json!({ "id": id }))))
}

/// PT: `GET /projects/{id}`.
pub async fn get(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<ProjectDetail>> {
    let project = sqlx::query_as!(
        Project,
        r#"SELECT p.id, p.name, p.description, p.created_at, p.updated_at,
                  (SELECT count(*) FROM project_blueprints pb WHERE pb.project_id = p.id) AS "blueprints!"
           FROM projects p WHERE p.id = $1 AND p.user_id = $2"#,
        id,
        user.id
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or(ApiError::NotFound)?;
    let items = sqlx::query_as!(
        ProjectBlueprint,
        "SELECT b.id, b.title, pb.position, pb.note
         FROM project_blueprints pb JOIN blueprints b ON b.id = pb.blueprint_id
         WHERE pb.project_id = $1 AND pb.user_id = $2 ORDER BY pb.position, b.id",
        id,
        user.id
    )
    .fetch_all(&state.db)
    .await?;
    Ok(Json(ProjectDetail { project, items }))
}

/// PT: `PATCH /projects/{id}`.
pub async fn update(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    Json(body): Json<ProjectBody>,
) -> ApiResult<StatusCode> {
    state.ensure_writable()?;
    state.check_write_rate(user.id)?;
    let name = clean_text(body.name.as_deref(), 120, "project.name_invalid")?;
    let description = clean_text(
        body.description.as_deref(),
        2000,
        "project.description_invalid",
    )?;
    let updated = sqlx::query!(
        "UPDATE projects SET name = COALESCE($3, name),
                             description = CASE WHEN $4::boolean THEN $5 ELSE description END,
                             updated_at = now()
         WHERE id = $1 AND user_id = $2",
        id,
        user.id,
        name,
        body.description.is_some(),
        description
    )
    .execute(&state.db)
    .await;
    match updated {
        Ok(result) if result.rows_affected() == 0 => Err(ApiError::NotFound),
        Ok(_) => Ok(StatusCode::NO_CONTENT),
        Err(sqlx::Error::Database(error)) if error.is_unique_violation() => {
            Err(ApiError::Conflict("project.name_taken"))
        }
        Err(error) => Err(error.into()),
    }
}

/// PT: `DELETE /projects/{id}`.
pub async fn delete(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<StatusCode> {
    state.check_write_rate(user.id)?;
    let mut tx = state.db.begin().await?;
    let deleted = sqlx::query!(
        "DELETE FROM projects WHERE id = $1 AND user_id = $2",
        id,
        user.id
    )
    .execute(&mut *tx)
    .await?;
    if deleted.rows_affected() == 0 {
        return Err(ApiError::NotFound);
    }
    sqlx::query!(
        "UPDATE user_usage SET projects = GREATEST(projects - 1, 0), updated_at = now() WHERE user_id = $1",
        user.id
    )
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AddBody {
    blueprint_id: Uuid,
    #[serde(default)]
    position: Option<i32>,
    #[serde(default)]
    note: Option<String>,
}

/// PT: `POST /projects/{id}/blueprints`. O banco recusa blueprint ou projeto de outro usuário.
/// EN: `POST /projects/{id}/blueprints`. The database refuses another user's blueprint or project.
pub async fn add_blueprint(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    Json(body): Json<AddBody>,
) -> ApiResult<StatusCode> {
    state.ensure_writable()?;
    state.check_write_rate(user.id)?;
    let note = clean_text(body.note.as_deref(), 500, "project.note_invalid")?;
    let position = body.position.unwrap_or(0).max(0);
    let mut tx = state.db.begin().await?;
    // PT: Trava o projeto para contar e inserir sem corrida. EN: Locks the project to count and insert without a race.
    sqlx::query_scalar!(
        "SELECT id FROM projects WHERE id = $1 AND user_id = $2 FOR UPDATE",
        id,
        user.id
    )
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(ApiError::NotFound)?;
    let count = sqlx::query_scalar!(
        r#"SELECT count(*) AS "count!" FROM project_blueprints WHERE project_id = $1"#,
        id
    )
    .fetch_one(&mut *tx)
    .await?;
    // PT: A cota só vale para itens novos; atualizar nota ou posição de um item existente sempre pode.
    // EN: The quota only applies to new items; updating an existing item's note or position is always allowed.
    let exists = sqlx::query_scalar!(
        r#"SELECT EXISTS (SELECT 1 FROM project_blueprints WHERE project_id = $1 AND blueprint_id = $2) AS "exists!""#,
        id,
        body.blueprint_id
    )
    .fetch_one(&mut *tx)
    .await?;
    if !exists && count >= state.config.quotas.blueprints_per_project {
        return Err(ApiError::Conflict("quota.exceeded"));
    }
    let inserted = sqlx::query!(
        "INSERT INTO project_blueprints (project_id, blueprint_id, user_id, position, note)
         VALUES ($1, $2, $3, $4, $5) ON CONFLICT (project_id, blueprint_id)
         DO UPDATE SET position = EXCLUDED.position, note = EXCLUDED.note",
        id,
        body.blueprint_id,
        user.id,
        position,
        note
    )
    .execute(&mut *tx)
    .await;
    match inserted {
        Ok(_) => {}
        Err(sqlx::Error::Database(error)) if error.is_foreign_key_violation() => {
            return Err(ApiError::NotFound);
        }
        Err(error) => return Err(error.into()),
    }
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

/// PT: `DELETE /projects/{id}/blueprints/{blueprint_id}`.
pub async fn remove_blueprint(
    State(state): State<AppState>,
    user: CurrentUser,
    Path((id, blueprint)): Path<(Uuid, Uuid)>,
) -> ApiResult<StatusCode> {
    state.check_write_rate(user.id)?;
    let deleted = sqlx::query!(
        "DELETE FROM project_blueprints WHERE project_id = $1 AND blueprint_id = $2 AND user_id = $3",
        id,
        blueprint,
        user.id
    )
    .execute(&state.db)
    .await?;
    if deleted.rows_affected() == 0 {
        return Err(ApiError::NotFound);
    }
    Ok(StatusCode::NO_CONTENT)
}
