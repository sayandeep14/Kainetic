//! Agent registry CRUD — `GET/POST /v1/agents`, `GET /v1/agents/:id`.

use axum::{
    extract::{Path, State},
    Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use uuid::Uuid;

use crate::{auth::AuthenticatedUser, error::CloudError, AppState};

/// Wire representation of an agent returned from the API.
#[derive(Debug, Serialize)]
pub struct AgentResponse {
    pub id: Uuid,
    pub team_id: Uuid,
    pub name: String,
    pub version: String,
    pub description: Option<String>,
    pub config: serde_json::Value,
    pub created_at: DateTime<Utc>,
}

/// Request body to register a new agent.
#[derive(Debug, Deserialize)]
pub struct CreateAgentRequest {
    pub name: String,
    pub version: Option<String>,
    pub description: Option<String>,
    pub config: Option<serde_json::Value>,
}

fn row_to_agent(row: &sqlx::postgres::PgRow) -> Result<AgentResponse, CloudError> {
    Ok(AgentResponse {
        id: row.try_get("id").map_err(|e| CloudError::Database(e.to_string()))?,
        team_id: row.try_get("team_id").map_err(|e| CloudError::Database(e.to_string()))?,
        name: row.try_get("name").map_err(|e| CloudError::Database(e.to_string()))?,
        version: row.try_get("version").map_err(|e| CloudError::Database(e.to_string()))?,
        description: row.try_get("description").map_err(|e| CloudError::Database(e.to_string()))?,
        config: row.try_get("config").map_err(|e| CloudError::Database(e.to_string()))?,
        created_at: row.try_get("created_at").map_err(|e| CloudError::Database(e.to_string()))?,
    })
}

/// `GET /v1/agents` — list all agents for the authenticated team.
pub async fn list_agents(
    State(state): State<AppState>,
    user: AuthenticatedUser,
) -> Result<Json<Vec<AgentResponse>>, CloudError> {
    let team_id: Uuid = user
        .team_id
        .parse()
        .map_err(|_| CloudError::BadRequest("invalid team_id".into()))?;

    let rows = sqlx::query(
        "SELECT id, team_id, name, version, description, config, created_at \
         FROM kc_agents WHERE team_id = $1 ORDER BY name, version",
    )
    .bind(team_id)
    .fetch_all(&state.pool)
    .await
    .map_err(|e| CloudError::Database(e.to_string()))?;

    rows.iter().map(row_to_agent).collect::<Result<Vec<_>, _>>().map(Json)
}

/// `POST /v1/agents` — register a new agent version.
pub async fn create_agent(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Json(body): Json<CreateAgentRequest>,
) -> Result<Json<AgentResponse>, CloudError> {
    user.require_write()?;

    if body.name.trim().is_empty() {
        return Err(CloudError::BadRequest("agent name must not be empty".into()));
    }

    let team_id: Uuid = user
        .team_id
        .parse()
        .map_err(|_| CloudError::BadRequest("invalid team_id".into()))?;

    let version = body.version.unwrap_or_else(|| "0.1.0".into());
    let config = body.config.unwrap_or_else(|| serde_json::json!({}));

    let row = sqlx::query(
        "INSERT INTO kc_agents (team_id, name, version, description, config) \
         VALUES ($1, $2, $3, $4, $5) \
         RETURNING id, team_id, name, version, description, config, created_at",
    )
    .bind(team_id)
    .bind(body.name.trim())
    .bind(&version)
    .bind(&body.description)
    .bind(&config)
    .fetch_one(&state.pool)
    .await
    .map_err(|e| {
        if e.to_string().contains("unique") {
            CloudError::BadRequest(format!(
                "agent '{}' version '{version}' already exists",
                body.name
            ))
        } else {
            CloudError::Database(e.to_string())
        }
    })?;

    row_to_agent(&row).map(Json)
}

/// `GET /v1/agents/:id` — fetch a single agent by UUID.
pub async fn get_agent(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Path(agent_id): Path<Uuid>,
) -> Result<Json<AgentResponse>, CloudError> {
    let team_id: Uuid = user
        .team_id
        .parse()
        .map_err(|_| CloudError::BadRequest("invalid team_id".into()))?;

    let row = sqlx::query(
        "SELECT id, team_id, name, version, description, config, created_at \
         FROM kc_agents WHERE id = $1 AND team_id = $2",
    )
    .bind(agent_id)
    .bind(team_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| CloudError::Database(e.to_string()))?
    .ok_or_else(|| CloudError::NotFound(format!("agent {agent_id}")))?;

    row_to_agent(&row).map(Json)
}
