//! # Kainetic Cloud
//!
//! Production backend for the Kainetic managed platform.  Provides:
//!
//! - **Agent registry** — register, version, and retrieve agent definitions
//! - **Run ingestion** — store run records and fine-grained OTLP spans
//! - **Metrics API** — aggregated cost and latency statistics
//! - **Auth** — API-key (argon2) + JWT (RS256/HS256) with RBAC
//! - **Cost alerts** — configurable thresholds with webhook / email delivery
//! - **Audit log** — tamper-evident HMAC-chained event log
//! - **Deployments** — track running agent deployments

#![deny(unsafe_code)]
#![warn(missing_docs)]

#[allow(missing_docs)]
pub mod api;
pub mod auth;
pub mod config;
pub mod db;
pub mod error;
pub mod server;

pub use config::CloudConfig;
pub use error::CloudError;

use std::sync::Arc;

use sqlx::PgPool;

/// Shared state injected into every Axum route handler via [`axum::extract::State`].
#[derive(Clone)]
pub struct AppState {
    /// Active database connection pool.
    pub pool: PgPool,
    /// Resolved runtime configuration.
    pub config: Arc<CloudConfig>,
}

impl AppState {
    /// Creates a new `AppState`.
    #[must_use]
    pub fn new(pool: PgPool, config: CloudConfig) -> Self {
        Self {
            pool,
            config: Arc::new(config),
        }
    }
}
