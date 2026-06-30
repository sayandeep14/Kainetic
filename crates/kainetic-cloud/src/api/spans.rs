//! `POST /v1/ingest/spans` — batch span ingestion.

use axum::{extract::State, Json};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{auth::AuthenticatedUser, error::CloudError, AppState};

/// A single span in the ingest payload.
#[derive(Debug, Deserialize)]
pub struct IngestSpan {
    pub id: Uuid,
    pub run_id: Uuid,
    pub parent_span_id: Option<Uuid>,
    pub name: String,
    #[serde(default = "default_kind")]
    pub kind: String,
    #[serde(default = "default_status")]
    pub status: String,
    pub start_time: DateTime<Utc>,
    pub end_time: Option<DateTime<Utc>>,
    #[serde(default)]
    pub attributes: serde_json::Value,
    #[serde(default = "default_events")]
    pub events: serde_json::Value,
}

fn default_kind() -> String {
    "internal".into()
}
fn default_status() -> String {
    "ok".into()
}
fn default_events() -> serde_json::Value {
    serde_json::json!([])
}

/// Request body for the ingest endpoint.
#[derive(Debug, Deserialize)]
pub struct IngestRequest {
    pub spans: Vec<IngestSpan>,
}

/// Ingest response.
#[derive(Debug, Serialize)]
pub struct IngestResponse {
    pub accepted: usize,
}

/// `POST /v1/ingest/spans`
pub async fn post_spans(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Json(body): Json<IngestRequest>,
) -> Result<Json<IngestResponse>, CloudError> {
    if body.spans.is_empty() {
        return Err(CloudError::BadRequest(
            "spans array must not be empty".into(),
        ));
    }

    let team_id: Uuid = user
        .team_id
        .parse()
        .map_err(|_| CloudError::BadRequest("invalid team_id in token".into()))?;

    let mut accepted = 0_usize;

    for span in &body.spans {
        sqlx::query(
            "INSERT INTO kc_spans \
             (id, run_id, team_id, parent_span_id, name, kind, status, \
              start_time, end_time, attributes, events) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11) \
             ON CONFLICT (id) DO NOTHING",
        )
        .bind(span.id)
        .bind(span.run_id)
        .bind(team_id)
        .bind(span.parent_span_id)
        .bind(&span.name)
        .bind(&span.kind)
        .bind(&span.status)
        .bind(span.start_time)
        .bind(span.end_time)
        .bind(&span.attributes)
        .bind(&span.events)
        .execute(&state.pool)
        .await
        .map_err(|e| CloudError::Database(e.to_string()))?;

        accepted += 1;
    }

    Ok(Json(IngestResponse { accepted }))
}
