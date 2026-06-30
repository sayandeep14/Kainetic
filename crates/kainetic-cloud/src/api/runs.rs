//! Run query API — `GET /v1/runs`, `POST /v1/runs`, `GET /v1/runs/:id`.

use axum::{
    extract::{Path, Query, State},
    Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use uuid::Uuid;

use crate::{auth::AuthenticatedUser, error::CloudError, AppState};

/// A run record returned from the API.
#[derive(Debug, Serialize)]
pub struct RunResponse {
    pub id: Uuid,
    pub team_id: Uuid,
    pub agent_id: Option<Uuid>,
    pub agent_name: String,
    pub status: String,
    pub input_preview: Option<String>,
    pub output_preview: Option<String>,
    pub error_message: Option<String>,
    pub prompt_tokens: i32,
    pub completion_tokens: i32,
    pub total_cost_usd: f64,
    pub duration_ms: Option<i32>,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub metadata: serde_json::Value,
}

fn row_to_run(row: &sqlx::postgres::PgRow) -> Result<RunResponse, CloudError> {
    Ok(RunResponse {
        id: row
            .try_get("id")
            .map_err(|e| CloudError::Database(e.to_string()))?,
        team_id: row
            .try_get("team_id")
            .map_err(|e| CloudError::Database(e.to_string()))?,
        agent_id: row
            .try_get("agent_id")
            .map_err(|e| CloudError::Database(e.to_string()))?,
        agent_name: row
            .try_get("agent_name")
            .map_err(|e| CloudError::Database(e.to_string()))?,
        status: row
            .try_get("status")
            .map_err(|e| CloudError::Database(e.to_string()))?,
        input_preview: row
            .try_get("input_preview")
            .map_err(|e| CloudError::Database(e.to_string()))?,
        output_preview: row
            .try_get("output_preview")
            .map_err(|e| CloudError::Database(e.to_string()))?,
        error_message: row
            .try_get("error_message")
            .map_err(|e| CloudError::Database(e.to_string()))?,
        prompt_tokens: row
            .try_get("prompt_tokens")
            .map_err(|e| CloudError::Database(e.to_string()))?,
        completion_tokens: row
            .try_get("completion_tokens")
            .map_err(|e| CloudError::Database(e.to_string()))?,
        total_cost_usd: row
            .try_get("total_cost_usd")
            .map_err(|e| CloudError::Database(e.to_string()))?,
        duration_ms: row
            .try_get("duration_ms")
            .map_err(|e| CloudError::Database(e.to_string()))?,
        started_at: row
            .try_get("started_at")
            .map_err(|e| CloudError::Database(e.to_string()))?,
        completed_at: row
            .try_get("completed_at")
            .map_err(|e| CloudError::Database(e.to_string()))?,
        metadata: row
            .try_get("metadata")
            .map_err(|e| CloudError::Database(e.to_string()))?,
    })
}

const RUN_COLS: &str = "id, team_id, agent_id, agent_name, status, input_preview, \
    output_preview, error_message, prompt_tokens, completion_tokens, \
    total_cost_usd, duration_ms, started_at, completed_at, metadata";

/// Query parameters for `GET /v1/runs`.
#[derive(Debug, Deserialize)]
pub struct RunsFilter {
    pub agent_name: Option<String>,
    pub status: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

/// `GET /v1/runs` — list recent runs with optional filters.
pub async fn list_runs(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Query(filter): Query<RunsFilter>,
) -> Result<Json<Vec<RunResponse>>, CloudError> {
    let team_id: Uuid = user
        .team_id
        .parse()
        .map_err(|_| CloudError::BadRequest("invalid team_id".into()))?;

    let limit = filter.limit.unwrap_or(50).min(200);
    let offset = filter.offset.unwrap_or(0);

    // Build dynamic SQL via string construction (no macro needed).
    let (where_extra, binds) = match (&filter.agent_name, &filter.status) {
        (Some(a), Some(s)) => (
            " AND agent_name = $2 AND status = $3 ORDER BY started_at DESC LIMIT $4 OFFSET $5",
            vec![a.clone(), s.clone()],
        ),
        (Some(a), None) => (
            " AND agent_name = $2 ORDER BY started_at DESC LIMIT $3 OFFSET $4",
            vec![a.clone()],
        ),
        (None, Some(s)) => (
            " AND status = $2 ORDER BY started_at DESC LIMIT $3 OFFSET $4",
            vec![s.clone()],
        ),
        (None, None) => (" ORDER BY started_at DESC LIMIT $2 OFFSET $3", vec![]),
    };

    let sql = format!("SELECT {RUN_COLS} FROM kc_runs WHERE team_id = $1{where_extra}");

    let mut q = sqlx::query(&sql).bind(team_id);
    for b in &binds {
        q = q.bind(b);
    }
    q = q.bind(limit).bind(offset);

    let rows = q
        .fetch_all(&state.pool)
        .await
        .map_err(|e| CloudError::Database(e.to_string()))?;

    rows.iter()
        .map(row_to_run)
        .collect::<Result<Vec<_>, _>>()
        .map(Json)
}

/// Request body for submitting a new run record.
#[derive(Debug, Deserialize)]
pub struct CreateRunRequest {
    pub id: Uuid,
    pub agent_id: Option<Uuid>,
    pub agent_name: String,
    pub status: String,
    pub input_preview: Option<String>,
    pub started_at: DateTime<Utc>,
    pub metadata: Option<serde_json::Value>,
}

/// `POST /v1/runs` — submit a new run record.
pub async fn create_run(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Json(body): Json<CreateRunRequest>,
) -> Result<Json<RunResponse>, CloudError> {
    user.require_write()?;

    let valid_statuses = ["running", "completed", "failed", "cancelled"];
    if !valid_statuses.contains(&body.status.as_str()) {
        return Err(CloudError::BadRequest(format!(
            "invalid status '{}'; expected one of {valid_statuses:?}",
            body.status
        )));
    }

    let team_id: Uuid = user
        .team_id
        .parse()
        .map_err(|_| CloudError::BadRequest("invalid team_id".into()))?;

    let input_preview = body
        .input_preview
        .as_deref()
        .map(|s| s.chars().take(500).collect::<String>());

    let metadata = body.metadata.unwrap_or_else(|| serde_json::json!({}));

    let row = sqlx::query(&format!(
        "INSERT INTO kc_runs \
             (id, team_id, agent_id, agent_name, status, input_preview, started_at, metadata) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8) \
             RETURNING {RUN_COLS}"
    ))
    .bind(body.id)
    .bind(team_id)
    .bind(body.agent_id)
    .bind(&body.agent_name)
    .bind(&body.status)
    .bind(&input_preview)
    .bind(body.started_at)
    .bind(&metadata)
    .fetch_one(&state.pool)
    .await
    .map_err(|e| CloudError::Database(e.to_string()))?;

    row_to_run(&row).map(Json)
}

/// `GET /v1/runs/:id` — fetch a single run by UUID.
pub async fn get_run(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Path(run_id): Path<Uuid>,
) -> Result<Json<RunResponse>, CloudError> {
    let team_id: Uuid = user
        .team_id
        .parse()
        .map_err(|_| CloudError::BadRequest("invalid team_id".into()))?;

    let row = sqlx::query(&format!(
        "SELECT {RUN_COLS} FROM kc_runs WHERE id = $1 AND team_id = $2"
    ))
    .bind(run_id)
    .bind(team_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| CloudError::Database(e.to_string()))?
    .ok_or_else(|| CloudError::NotFound(format!("run {run_id}")))?;

    row_to_run(&row).map(Json)
}
