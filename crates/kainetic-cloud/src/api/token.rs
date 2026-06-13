//! `POST /v1/auth/token` — exchange API key for a short-lived JWT.

use axum::{extract::State, Json};
use serde::{Deserialize, Serialize};
use sqlx::Row;

use crate::{
    auth::{
        api_key,
        jwt::{self, Role},
    },
    error::CloudError,
    AppState,
};

/// Request body for token exchange.
#[derive(Debug, Deserialize)]
pub struct TokenRequest {
    pub api_key: String,
}

/// Response body containing the JWT.
#[derive(Debug, Serialize)]
pub struct TokenResponse {
    pub access_token: String,
    pub token_type: String,
    pub expires_in: u64,
}

/// `POST /v1/auth/token`
///
/// Exchanges a valid API key for a short-lived JWT.
pub async fn post_token(
    State(state): State<AppState>,
    Json(body): Json<TokenRequest>,
) -> Result<Json<TokenResponse>, CloudError> {
    let prefix = if body.api_key.len() >= 8 {
        &body.api_key[..8]
    } else {
        return Err(CloudError::Unauthorized("invalid api key".into()));
    };

    let rows = sqlx::query(
        "SELECT ak.key_hash, ak.team_id::TEXT AS team_id, \
               tm.user_id::TEXT AS user_id, tm.role \
         FROM kc_api_keys ak \
         JOIN kc_team_members tm ON tm.team_id = ak.team_id \
         WHERE ak.prefix = $1",
    )
    .bind(prefix)
    .fetch_all(&state.pool)
    .await
    .map_err(|e| CloudError::Database(e.to_string()))?;

    for row in &rows {
        let key_hash: String = row
            .try_get("key_hash")
            .map_err(|e| CloudError::Database(e.to_string()))?;

        if api_key::verify(&body.api_key, &key_hash)? {
            let role_str: String = row
                .try_get("role")
                .map_err(|e| CloudError::Database(e.to_string()))?;
            let role = match role_str.as_str() {
                "admin" => Role::Admin,
                "developer" => Role::Developer,
                _ => Role::Viewer,
            };
            let user_id: String = row.try_get("user_id").unwrap_or_default();
            let team_id: String = row.try_get("team_id").unwrap_or_default();

            let ttl = state.config.jwt_ttl.as_secs();
            let claims = jwt::build_claims(&user_id, &team_id, role, ttl);
            let token = jwt::encode_token(&claims, &state.config.jwt_secret)?;

            return Ok(Json(TokenResponse {
                access_token: token,
                token_type: "Bearer".into(),
                expires_in: ttl,
            }));
        }
    }

    Err(CloudError::Unauthorized("invalid api key".into()))
}
