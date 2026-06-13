//! Database access layer for Kainetic Cloud.

pub mod schema;

use sqlx::PgPool;

use crate::error::CloudError;
use schema::SCHEMA_SQL;

/// Runs `CREATE TABLE IF NOT EXISTS` for every Kainetic Cloud table.
///
/// Safe to call on every startup.
///
/// # Errors
///
/// Returns [`CloudError::Database`] if any statement fails.
pub async fn migrate(pool: &PgPool) -> Result<(), CloudError> {
    sqlx::query(SCHEMA_SQL)
        .execute(pool)
        .await
        .map_err(|e| CloudError::Database(e.to_string()))?;
    Ok(())
}
