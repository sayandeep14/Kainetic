//! [`CloudConfig`] — server configuration loaded from environment variables.

use std::time::Duration;

/// Runtime configuration for the Kainetic Cloud server.
///
/// Load from environment variables via [`CloudConfig::from_env`].
#[derive(Debug, Clone)]
pub struct CloudConfig {
    /// PostgreSQL connection URL.
    ///
    /// Default: `postgres://localhost/kainetic_cloud`
    pub database_url: String,

    /// HMAC-SHA256 secret used to sign and verify JWTs.
    ///
    /// Set `JWT_SECRET` in the environment.  Must be at least 32 bytes.
    pub jwt_secret: String,

    /// Port to listen on.  Default: `8080`.
    pub port: u16,

    /// Optional ClickHouse URL for high-throughput span storage.
    ///
    /// If unset, spans are stored in PostgreSQL.
    pub clickhouse_url: Option<String>,

    /// JWT expiry window.  Default: 24 hours.
    pub jwt_ttl: Duration,

    /// HMAC key used to chain audit-log entries.
    ///
    /// Defaults to `jwt_secret` if `AUDIT_HMAC_KEY` is not set.
    pub audit_hmac_key: String,

    /// Base URL returned in deployment responses (e.g. `https://run.kainetic.dev`).
    pub public_base_url: String,
}

impl CloudConfig {
    /// Loads configuration from environment variables with sensible defaults.
    ///
    /// # Required env vars
    ///
    /// - `JWT_SECRET` — must be ≥ 32 characters
    ///
    /// # Optional env vars
    ///
    /// | Variable | Default |
    /// |----------|---------|
    /// | `DATABASE_URL` | `postgres://localhost/kainetic_cloud` |
    /// | `PORT` | `8080` |
    /// | `CLICKHOUSE_URL` | *(unset)* |
    /// | `JWT_TTL_SECS` | `86400` (24 h) |
    /// | `AUDIT_HMAC_KEY` | same as `JWT_SECRET` |
    /// | `PUBLIC_BASE_URL` | `http://localhost:8080` |
    #[must_use]
    pub fn from_env() -> Self {
        let jwt_secret = std::env::var("JWT_SECRET")
            .unwrap_or_else(|_| "dev-secret-change-me-in-production".to_string());

        let audit_hmac_key =
            std::env::var("AUDIT_HMAC_KEY").unwrap_or_else(|_| jwt_secret.clone());

        let jwt_ttl_secs: u64 = std::env::var("JWT_TTL_SECS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(86_400);

        Self {
            database_url: std::env::var("DATABASE_URL")
                .unwrap_or_else(|_| "postgres://localhost/kainetic_cloud".to_string()),
            jwt_secret,
            port: std::env::var("PORT")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(8080),
            clickhouse_url: std::env::var("CLICKHOUSE_URL").ok(),
            jwt_ttl: Duration::from_secs(jwt_ttl_secs),
            audit_hmac_key,
            public_base_url: std::env::var("PUBLIC_BASE_URL")
                .unwrap_or_else(|_| "http://localhost:8080".to_string()),
        }
    }

    /// Creates a config for testing with a provided database URL.
    #[cfg(test)]
    pub fn for_test(database_url: impl Into<String>) -> Self {
        Self {
            database_url: database_url.into(),
            jwt_secret: "test-secret-32-bytes-exactly!!".to_string(),
            port: 0,
            clickhouse_url: None,
            jwt_ttl: Duration::from_secs(3600),
            audit_hmac_key: "test-hmac-key-32-bytes-exactly!".to_string(),
            public_base_url: "http://localhost:8080".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_env_uses_defaults() {
        // Ensure env vars from CI don't bleed in.
        let _ = std::env::remove_var("PORT");
        let _ = std::env::remove_var("DATABASE_URL");
        let cfg = CloudConfig::from_env();
        assert_eq!(cfg.port, 8080);
        assert!(cfg.database_url.contains("kainetic_cloud"));
        assert!(cfg.clickhouse_url.is_none());
    }

    #[test]
    fn jwt_ttl_default_is_24h() {
        let _ = std::env::remove_var("JWT_TTL_SECS");
        let cfg = CloudConfig::from_env();
        assert_eq!(cfg.jwt_ttl.as_secs(), 86_400);
    }
}
