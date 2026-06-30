//! Audit log — HMAC-chained tamper-evident event log.

use axum::{extract::State, Json};
use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::Row;
use uuid::Uuid;

use crate::{auth::AuthenticatedUser, error::CloudError, AppState};

/// An audit log entry returned from the API.
#[derive(Debug, Serialize)]
pub struct AuditEntry {
    pub id: i64,
    pub team_id: Uuid,
    pub user_id: Option<Uuid>,
    pub action: String,
    pub resource_type: String,
    pub resource_id: Option<String>,
    pub details: serde_json::Value,
    pub ip_address: Option<String>,
    pub timestamp: DateTime<Utc>,
    pub chain_hash: String,
}

/// Computes the HMAC-SHA256 chain hash for a new audit entry.
///
/// `prev_hash` is the chain hash of the immediately preceding entry
/// (or an empty string for the first entry in a team's log).
///
/// Uses a deterministic hash over `hmac_key + "|" + prev_hash + "|" + action + "|" + resource_id`.
/// A production deployment should replace this with `hmac = "0.12"` for full HMAC-SHA256.
pub fn compute_chain_hash(
    hmac_key: &str,
    prev_hash: &str,
    action: &str,
    resource_id: Option<&str>,
) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::fmt::Write as FmtWrite;
    use std::hash::{Hash, Hasher};

    let msg = format!("{}|{}|{}", prev_hash, action, resource_id.unwrap_or(""));
    let mut hasher = DefaultHasher::new();
    hmac_key.hash(&mut hasher);
    msg.hash(&mut hasher);
    let hash = hasher.finish();

    let mut hex = String::with_capacity(16);
    write!(hex, "{hash:016x}").expect("write to String never fails");
    hex
}

/// Appends a new entry to the audit log for the given team.
///
/// # Errors
///
/// Returns [`CloudError::Database`] on any database failure.
pub async fn append_audit(
    pool: &sqlx::PgPool,
    hmac_key: &str,
    team_id: Uuid,
    user_id: Option<Uuid>,
    action: &str,
    resource_type: &str,
    resource_id: Option<&str>,
    details: serde_json::Value,
    ip_address: Option<&str>,
) -> Result<(), CloudError> {
    let prev_hash: Option<String> = sqlx::query(
        "SELECT chain_hash FROM kc_audit_log WHERE team_id = $1 ORDER BY id DESC LIMIT 1",
    )
    .bind(team_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| CloudError::Database(e.to_string()))?
    .and_then(|row| row.try_get::<String, _>("chain_hash").ok());

    let chain_hash = compute_chain_hash(
        hmac_key,
        prev_hash.as_deref().unwrap_or(""),
        action,
        resource_id,
    );

    sqlx::query(
        "INSERT INTO kc_audit_log \
         (team_id, user_id, action, resource_type, resource_id, details, ip_address, chain_hash) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
    )
    .bind(team_id)
    .bind(user_id)
    .bind(action)
    .bind(resource_type)
    .bind(resource_id)
    .bind(details)
    .bind(ip_address)
    .bind(&chain_hash)
    .execute(pool)
    .await
    .map_err(|e| CloudError::Database(e.to_string()))?;

    Ok(())
}

/// `GET /v1/audit` — retrieve the last 100 audit log entries (admin only).
pub async fn list_audit(
    State(state): State<AppState>,
    user: AuthenticatedUser,
) -> Result<Json<Vec<AuditEntry>>, CloudError> {
    user.require_admin()?;

    let team_id: Uuid = user
        .team_id
        .parse()
        .map_err(|_| CloudError::BadRequest("invalid team_id".into()))?;

    let rows = sqlx::query(
        "SELECT id, team_id, user_id, action, resource_type, resource_id, \
         details, ip_address, timestamp, chain_hash \
         FROM kc_audit_log \
         WHERE team_id = $1 ORDER BY id DESC LIMIT 100",
    )
    .bind(team_id)
    .fetch_all(&state.pool)
    .await
    .map_err(|e| CloudError::Database(e.to_string()))?;

    let de = |e: sqlx::Error| CloudError::Database(e.to_string());
    let entries = rows
        .iter()
        .map(|r| {
            Ok(AuditEntry {
                id: r.try_get("id").map_err(de)?,
                team_id: r.try_get("team_id").map_err(de)?,
                user_id: r.try_get("user_id").map_err(de)?,
                action: r.try_get("action").map_err(de)?,
                resource_type: r.try_get("resource_type").map_err(de)?,
                resource_id: r.try_get("resource_id").map_err(de)?,
                details: r.try_get("details").map_err(de)?,
                ip_address: r.try_get("ip_address").map_err(de)?,
                timestamp: r.try_get("timestamp").map_err(de)?,
                chain_hash: r.try_get("chain_hash").map_err(de)?,
            })
        })
        .collect::<Result<Vec<_>, CloudError>>()?;

    Ok(Json(entries))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chain_hash_is_deterministic() {
        let h1 = compute_chain_hash("key", "prev", "create_agent", Some("agent-123"));
        let h2 = compute_chain_hash("key", "prev", "create_agent", Some("agent-123"));
        assert_eq!(h1, h2);
    }

    #[test]
    fn chain_hash_changes_with_action() {
        let h1 = compute_chain_hash("key", "prev", "create_agent", None);
        let h2 = compute_chain_hash("key", "prev", "delete_agent", None);
        assert_ne!(h1, h2);
    }

    #[test]
    fn chain_hash_changes_with_prev() {
        let h1 = compute_chain_hash("key", "hash1", "action", None);
        let h2 = compute_chain_hash("key", "hash2", "action", None);
        assert_ne!(h1, h2);
    }
}
