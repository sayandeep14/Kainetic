//! Entry point for the Kainetic Cloud backend server.
//!
//! # Usage
//!
//! ```text
//! kainetic-cloud
//! ```
//!
//! Configuration is loaded from environment variables.  See
//! [`kainetic_cloud::CloudConfig`] for the full list.

use std::net::SocketAddr;

use sqlx::postgres::PgPoolOptions;
use tracing::info;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

use kainetic_cloud::{db, server, AppState, CloudConfig};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Structured logging — respects RUST_LOG env var.
    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .with(fmt::layer())
        .init();

    let config = CloudConfig::from_env();

    info!(port = config.port, "starting kainetic-cloud");

    // Connect to PostgreSQL.
    let pool = PgPoolOptions::new()
        .max_connections(20)
        .connect(&config.database_url)
        .await?;

    // Apply schema migrations (idempotent — safe on every startup).
    db::migrate(&pool).await?;

    let addr = SocketAddr::from(([0, 0, 0, 0], config.port));
    let state = AppState::new(pool, config);
    let router = server::build_router(state);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    info!(%addr, "listening");

    axum::serve(listener, router).await?;

    Ok(())
}
