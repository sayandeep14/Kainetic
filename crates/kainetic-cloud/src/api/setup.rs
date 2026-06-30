//! `POST /v1/setup` — one-time bootstrap to create the first admin user, team,
//! and API key.
//!
//! This endpoint is **only available when the `kc_users` table is empty**.
//! Once any user exists it returns `409 Conflict`.  Use it to bootstrap a fresh
//! installation without needing direct database access.
//!
//! # Example
//!
//! ```bash
//! curl -X POST http://localhost:8080/v1/setup \
//!   -H 'Content-Type: application/json' \
//!   -d '{"email": "admin@example.com", "team_name": "My Team"}'
//! ```

use axum::{extract::State, Json};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use uuid::Uuid;

use crate::{auth::api_key, error::CloudError, AppState};

/// Request body for the one-time setup endpoint.
#[derive(Debug, Deserialize)]
pub struct SetupRequest {
    /// E-mail address for the initial admin user (used as a display identifier).
    pub email: String,
    /// Display name for the initial team.
    pub team_name: String,
}

/// Response body returned by the setup endpoint.
#[derive(Debug, Serialize)]
pub struct SetupResponse {
    /// UUID of the newly created team — needed to log in via the dashboard.
    pub team_id: Uuid,
    /// UUID of the newly created admin user.
    pub user_id: Uuid,
    /// The plaintext API key.  **Shown only once — save it immediately.**
    pub api_key: String,
    /// When the team was created.
    pub created_at: DateTime<Utc>,
}

/// `POST /v1/setup`
///
/// Creates the first admin user, team, and API key in a single transaction.
/// Returns `409 Conflict` if any user already exists.
pub async fn post_setup(
    State(state): State<AppState>,
    Json(body): Json<SetupRequest>,
) -> Result<Json<SetupResponse>, CloudError> {
    if body.email.trim().is_empty() {
        return Err(CloudError::BadRequest("email must not be empty".into()));
    }
    if body.team_name.trim().is_empty() {
        return Err(CloudError::BadRequest("team_name must not be empty".into()));
    }

    let de = |e: sqlx::Error| CloudError::Database(e.to_string());

    // Guard: refuse if any user already exists.
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM kc_users")
        .fetch_one(&state.pool)
        .await
        .map_err(de)?;

    if count > 0 {
        return Err(CloudError::Conflict(
            "setup has already been completed — use the dashboard to manage users and API keys"
                .into(),
        ));
    }

    let mut tx = state.pool.begin().await.map_err(de)?;

    // 1. Create user.
    let user_row =
        sqlx::query("INSERT INTO kc_users (email) VALUES ($1) RETURNING id, created_at")
            .bind(body.email.trim())
            .fetch_one(&mut *tx)
            .await
            .map_err(de)?;
    let user_id: Uuid = user_row.try_get("id").map_err(de)?;

    // 2. Create team.
    let team_row =
        sqlx::query("INSERT INTO kc_teams (name) VALUES ($1) RETURNING id, created_at")
            .bind(body.team_name.trim())
            .fetch_one(&mut *tx)
            .await
            .map_err(de)?;
    let team_id: Uuid = team_row.try_get("id").map_err(de)?;
    let created_at: DateTime<Utc> = team_row.try_get("created_at").map_err(de)?;

    // 3. Make the user an admin of the team.
    sqlx::query(
        "INSERT INTO kc_team_members (team_id, user_id, role) VALUES ($1, $2, 'admin')",
    )
    .bind(team_id)
    .bind(user_id)
    .execute(&mut *tx)
    .await
    .map_err(de)?;

    // 4. Generate and store the initial API key.
    let generated = api_key::generate()?;
    sqlx::query(
        "INSERT INTO kc_api_keys (team_id, name, key_hash, prefix) \
         VALUES ($1, 'default', $2, $3)",
    )
    .bind(team_id)
    .bind(&generated.hash)
    .bind(&generated.prefix)
    .execute(&mut *tx)
    .await
    .map_err(de)?;

    tx.commit().await.map_err(de)?;

    Ok(Json(SetupResponse {
        team_id,
        user_id,
        api_key: generated.plaintext,
        created_at,
    }))
}
