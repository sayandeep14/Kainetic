//! Authentication and authorisation for Kainetic Cloud.
//!
//! Two auth strategies are supported:
//!
//! 1. **API key** (`Authorization: Bearer kk_…`) — long-lived keys stored as
//!    argon2 hashes; looked up from `kc_api_keys`.
//! 2. **JWT** (`Authorization: Bearer <jwt>`) — short-lived session tokens
//!    issued by `POST /v1/auth/token`.

pub mod api_key;
pub mod jwt;

use axum::{
    async_trait,
    extract::FromRequestParts,
    http::{request::Parts, HeaderMap},
};
use sqlx::{PgPool, Row};

pub use jwt::{Claims, Role};

use crate::{error::CloudError, AppState};

/// The resolved identity attached to a request after successful authentication.
#[derive(Debug, Clone)]
pub struct AuthenticatedUser {
    /// The authenticated user's UUID.
    pub user_id: String,
    /// The team UUID this request is scoped to.
    pub team_id: String,
    /// The caller's RBAC role within that team.
    pub role: Role,
}

impl AuthenticatedUser {
    /// Returns `Err(CloudError::Forbidden)` if the caller's role is not `admin`.
    ///
    /// # Errors
    ///
    /// Returns [`CloudError::Forbidden`] when the role is not [`Role::Admin`].
    pub fn require_admin(&self) -> Result<(), CloudError> {
        if self.role == Role::Admin {
            Ok(())
        } else {
            Err(CloudError::Forbidden("admin role required".into()))
        }
    }

    /// Returns `Err(CloudError::Forbidden)` if the caller is viewer-only.
    ///
    /// # Errors
    ///
    /// Returns [`CloudError::Forbidden`] when the role is [`Role::Viewer`].
    pub fn require_write(&self) -> Result<(), CloudError> {
        if self.role == Role::Viewer {
            Err(CloudError::Forbidden(
                "viewer role cannot perform write operations".into(),
            ))
        } else {
            Ok(())
        }
    }
}

fn extract_bearer(headers: &HeaderMap) -> Option<&str> {
    headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
}

async fn verify_api_key(
    pool: &PgPool,
    key: &str,
) -> Result<Option<(String, String, Role)>, CloudError> {
    if key.len() < 8 {
        return Ok(None);
    }
    let prefix = &key[..8];

    let rows = sqlx::query(
        "SELECT ak.key_hash, ak.team_id::TEXT AS team_id, \
               tm.user_id::TEXT AS user_id, tm.role \
         FROM kc_api_keys ak \
         JOIN kc_team_members tm ON tm.team_id = ak.team_id \
         WHERE ak.prefix = $1",
    )
    .bind(prefix)
    .fetch_all(pool)
    .await
    .map_err(|e| CloudError::Database(e.to_string()))?;

    for row in &rows {
        let key_hash: String = row
            .try_get("key_hash")
            .map_err(|e| CloudError::Database(e.to_string()))?;

        if api_key::verify(key, &key_hash)? {
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

            let _ = sqlx::query("UPDATE kc_api_keys SET last_used_at = NOW() WHERE prefix = $1")
                .bind(prefix)
                .execute(pool)
                .await;

            return Ok(Some((user_id, team_id, role)));
        }
    }

    Ok(None)
}

#[async_trait]
impl FromRequestParts<AppState> for AuthenticatedUser {
    type Rejection = CloudError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let token = extract_bearer(&parts.headers)
            .ok_or_else(|| CloudError::Unauthorized("missing Authorization header".into()))?;

        if let Ok(claims) = jwt::decode_token(token, &state.config.jwt_secret) {
            return Ok(AuthenticatedUser {
                user_id: claims.sub,
                team_id: claims.team_id,
                role: claims.role,
            });
        }

        if let Some((user_id, team_id, role)) = verify_api_key(&state.pool, token).await? {
            return Ok(AuthenticatedUser {
                user_id,
                team_id,
                role,
            });
        }

        Err(CloudError::Unauthorized("invalid credentials".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn require_admin_passes_for_admin() {
        let user = AuthenticatedUser {
            user_id: "u".into(),
            team_id: "t".into(),
            role: Role::Admin,
        };
        assert!(user.require_admin().is_ok());
    }

    #[test]
    fn require_admin_fails_for_developer() {
        let user = AuthenticatedUser {
            user_id: "u".into(),
            team_id: "t".into(),
            role: Role::Developer,
        };
        assert!(matches!(
            user.require_admin(),
            Err(CloudError::Forbidden(_))
        ));
    }

    #[test]
    fn require_write_passes_for_developer() {
        let user = AuthenticatedUser {
            user_id: "u".into(),
            team_id: "t".into(),
            role: Role::Developer,
        };
        assert!(user.require_write().is_ok());
    }

    #[test]
    fn require_write_fails_for_viewer() {
        let user = AuthenticatedUser {
            user_id: "u".into(),
            team_id: "t".into(),
            role: Role::Viewer,
        };
        assert!(matches!(
            user.require_write(),
            Err(CloudError::Forbidden(_))
        ));
    }

    #[test]
    fn extract_bearer_parses_header() {
        let mut headers = axum::http::HeaderMap::new();
        headers.insert("authorization", "Bearer my-token-123".parse().unwrap());
        assert_eq!(extract_bearer(&headers), Some("my-token-123"));
    }

    #[test]
    fn extract_bearer_missing_returns_none() {
        let headers = axum::http::HeaderMap::new();
        assert!(extract_bearer(&headers).is_none());
    }
}
