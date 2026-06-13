//! Team management — `POST /v1/teams`, `GET /v1/teams/:id/members`,
//! `POST /v1/teams/:id/api-keys`.

use axum::{
    extract::{Path, State},
    Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use uuid::Uuid;

use crate::{
    auth::{api_key, AuthenticatedUser},
    error::CloudError,
    AppState,
};

// ── Create team ──────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct CreateTeamRequest {
    pub name: String,
}

#[derive(Debug, Serialize)]
pub struct TeamResponse {
    pub id: Uuid,
    pub name: String,
    pub created_at: DateTime<Utc>,
}

/// `POST /v1/teams` — create a new team and make the caller its admin.
pub async fn create_team(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Json(body): Json<CreateTeamRequest>,
) -> Result<Json<TeamResponse>, CloudError> {
    if body.name.trim().is_empty() {
        return Err(CloudError::BadRequest("team name must not be empty".into()));
    }

    let user_id: Uuid = user
        .user_id
        .parse()
        .map_err(|_| CloudError::BadRequest("invalid user_id in token".into()))?;

    let mut tx = state
        .pool
        .begin()
        .await
        .map_err(|e| CloudError::Database(e.to_string()))?;

    let team_row = sqlx::query(
        "INSERT INTO kc_teams (name) VALUES ($1) RETURNING id, name, created_at",
    )
    .bind(body.name.trim())
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| CloudError::Database(e.to_string()))?;

    let team_id: Uuid = team_row
        .try_get("id")
        .map_err(|e| CloudError::Database(e.to_string()))?;

    sqlx::query(
        "INSERT INTO kc_team_members (team_id, user_id, role) VALUES ($1, $2, 'admin')",
    )
    .bind(team_id)
    .bind(user_id)
    .execute(&mut *tx)
    .await
    .map_err(|e| CloudError::Database(e.to_string()))?;

    tx.commit()
        .await
        .map_err(|e| CloudError::Database(e.to_string()))?;

    Ok(Json(TeamResponse {
        id: team_id,
        name: team_row
            .try_get("name")
            .map_err(|e| CloudError::Database(e.to_string()))?,
        created_at: team_row
            .try_get("created_at")
            .map_err(|e| CloudError::Database(e.to_string()))?,
    }))
}

// ── List members ─────────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct MemberResponse {
    pub user_id: Uuid,
    pub email: String,
    pub role: String,
    pub joined_at: DateTime<Utc>,
}

/// `GET /v1/teams/:id/members` — list members of a team (admin only).
pub async fn list_members(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Path(team_id): Path<Uuid>,
) -> Result<Json<Vec<MemberResponse>>, CloudError> {
    user.require_admin()?;

    let rows = sqlx::query(
        "SELECT tm.user_id, u.email, tm.role, tm.joined_at \
         FROM kc_team_members tm \
         JOIN kc_users u ON u.id = tm.user_id \
         WHERE tm.team_id = $1 \
         ORDER BY tm.joined_at",
    )
    .bind(team_id)
    .fetch_all(&state.pool)
    .await
    .map_err(|e| CloudError::Database(e.to_string()))?;

    let de = |e: sqlx::Error| CloudError::Database(e.to_string());
    let members = rows
        .iter()
        .map(|r| {
            Ok(MemberResponse {
                user_id: r.try_get("user_id").map_err(de)?,
                email: r.try_get("email").map_err(de)?,
                role: r.try_get("role").map_err(de)?,
                joined_at: r.try_get("joined_at").map_err(de)?,
            })
        })
        .collect::<Result<Vec<_>, CloudError>>()?;

    Ok(Json(members))
}

// ── API key management ───────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct CreateApiKeyRequest {
    pub name: String,
}

#[derive(Debug, Serialize)]
pub struct CreateApiKeyResponse {
    pub id: Uuid,
    /// Plaintext key — shown once at creation.
    pub key: String,
    pub prefix: String,
    pub name: String,
    pub created_at: DateTime<Utc>,
}

/// `POST /v1/teams/:id/api-keys` — generate a new API key for the team (admin only).
pub async fn create_api_key(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Path(team_id): Path<Uuid>,
    Json(body): Json<CreateApiKeyRequest>,
) -> Result<Json<CreateApiKeyResponse>, CloudError> {
    user.require_admin()?;

    let generated = api_key::generate()?;

    let de = |e: sqlx::Error| CloudError::Database(e.to_string());
    let row = sqlx::query(
        "INSERT INTO kc_api_keys (team_id, name, key_hash, prefix) \
         VALUES ($1, $2, $3, $4) \
         RETURNING id, created_at",
    )
    .bind(team_id)
    .bind(&body.name)
    .bind(&generated.hash)
    .bind(&generated.prefix)
    .fetch_one(&state.pool)
    .await
    .map_err(|e| CloudError::Database(e.to_string()))?;

    Ok(Json(CreateApiKeyResponse {
        id: row.try_get("id").map_err(de)?,
        key: generated.plaintext,
        prefix: generated.prefix,
        name: body.name,
        created_at: row.try_get("created_at").map_err(de)?,
    }))
}
