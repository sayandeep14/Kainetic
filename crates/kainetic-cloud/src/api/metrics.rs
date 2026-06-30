//! `GET /v1/metrics` — aggregated cost and latency statistics.

use axum::{extract::State, Json};
use serde::Serialize;
use sqlx::Row;
use uuid::Uuid;

use crate::{auth::AuthenticatedUser, error::CloudError, AppState};

/// Aggregated run statistics for the authenticated team.
#[derive(Debug, Serialize)]
pub struct MetricsResponse {
    pub total_runs: i64,
    pub completed_runs: i64,
    pub failed_runs: i64,
    pub total_cost_usd: f64,
    pub avg_cost_usd: f64,
    pub avg_duration_ms: f64,
    pub total_prompt_tokens: i64,
    pub total_completion_tokens: i64,
}

/// `GET /v1/metrics` — aggregated statistics for the caller's team.
pub async fn get_metrics(
    State(state): State<AppState>,
    user: AuthenticatedUser,
) -> Result<Json<MetricsResponse>, CloudError> {
    let team_id: Uuid = user
        .team_id
        .parse()
        .map_err(|_| CloudError::BadRequest("invalid team_id".into()))?;

    let row = sqlx::query(
        r#"
        SELECT
            COUNT(*)                                         AS total_runs,
            COUNT(*) FILTER (WHERE status = 'completed')    AS completed_runs,
            COUNT(*) FILTER (WHERE status = 'failed')       AS failed_runs,
            COALESCE(SUM(total_cost_usd), 0.0)              AS total_cost_usd,
            COALESCE(AVG(total_cost_usd), 0.0)              AS avg_cost_usd,
            COALESCE((AVG(duration_ms) FILTER (WHERE status = 'completed'))::FLOAT8, 0.0) AS avg_duration_ms,
            COALESCE(SUM(prompt_tokens), 0)                 AS total_prompt_tokens,
            COALESCE(SUM(completion_tokens), 0)             AS total_completion_tokens
        FROM kc_runs
        WHERE team_id = $1
        "#,
    )
    .bind(team_id)
    .fetch_one(&state.pool)
    .await
    .map_err(|e| CloudError::Database(e.to_string()))?;

    // COUNT() returns i64 via sqlx for Postgres; SUM(integer) → Option<i64>.
    let de = |e: sqlx::Error| CloudError::Database(e.to_string());
    Ok(Json(MetricsResponse {
        total_runs: row.try_get::<i64, _>("total_runs").map_err(de)?,
        completed_runs: row.try_get::<i64, _>("completed_runs").map_err(de)?,
        failed_runs: row.try_get::<i64, _>("failed_runs").map_err(de)?,
        total_cost_usd: row.try_get::<f64, _>("total_cost_usd").map_err(de)?,
        avg_cost_usd: row.try_get::<f64, _>("avg_cost_usd").map_err(de)?,
        avg_duration_ms: row.try_get::<f64, _>("avg_duration_ms").map_err(de)?,
        total_prompt_tokens: row.try_get::<i64, _>("total_prompt_tokens").map_err(de)?,
        total_completion_tokens: row.try_get::<i64, _>("total_completion_tokens").map_err(de)?,
    }))
}
