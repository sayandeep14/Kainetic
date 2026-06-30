//! Axum router construction and tower-http middleware stack.

use axum::{
    routing::{get, post},
    Router,
};
use tower_http::{
    cors::Any,
    request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer},
    trace::TraceLayer,
};

use crate::{
    api::{agents, audit, metrics, runs, setup, spans, teams, token},
    AppState,
};

/// Builds the full Axum [`Router`] with all API routes and middleware layers.
///
/// # Route table
///
/// | Method | Path | Description |
/// |--------|------|-------------|
/// | `POST` | `/v1/setup` | One-time bootstrap — create first admin (fails if any user exists) |
/// | `POST` | `/v1/auth/token` | Exchange API key for JWT |
/// | `GET`  | `/v1/agents` | List agents |
/// | `POST` | `/v1/agents` | Register agent |
/// | `GET`  | `/v1/agents/:id` | Get agent |
/// | `GET`  | `/v1/runs` | List runs (with filters) |
/// | `POST` | `/v1/runs` | Submit run record |
/// | `GET`  | `/v1/runs/:id` | Get run |
/// | `POST` | `/v1/ingest/spans` | Ingest span batch |
/// | `GET`  | `/v1/metrics` | Aggregated stats |
/// | `POST` | `/v1/teams` | Create team |
/// | `GET`  | `/v1/teams/:id/members` | List team members |
/// | `POST` | `/v1/teams/:id/api-keys` | Generate API key |
/// | `GET`  | `/v1/audit` | Audit log (admin only) |
/// | `GET`  | `/healthz` | Health check |
#[must_use]
pub fn build_router(state: AppState) -> Router {
    let api = Router::new()
        // One-time bootstrap (only works when no users exist yet)
        .route("/setup", post(setup::post_setup))
        // Auth
        .route("/auth/token", post(token::post_token))
        // Agent registry
        .route("/agents", get(agents::list_agents).post(agents::create_agent))
        .route("/agents/:id", get(agents::get_agent))
        // Runs
        .route("/runs", get(runs::list_runs).post(runs::create_run))
        .route("/runs/:id", get(runs::get_run))
        // Span ingestion
        .route("/ingest/spans", post(spans::post_spans))
        // Metrics
        .route("/metrics", get(metrics::get_metrics))
        // Teams
        .route("/teams", post(teams::create_team))
        .route("/teams/:id/members", get(teams::list_members))
        .route("/teams/:id/api-keys", post(teams::create_api_key))
        // Audit log
        .route("/audit", get(audit::list_audit));

    Router::new()
        .nest("/v1", api)
        .route("/healthz", get(healthz))
        .layer(
            tower_http::cors::CorsLayer::new()
                .allow_origin(Any)
                .allow_headers(Any)
                .allow_methods(Any),
        )
        .layer(TraceLayer::new_for_http())
        .layer(PropagateRequestIdLayer::x_request_id())
        .layer(SetRequestIdLayer::x_request_id(MakeRequestUuid))
        .with_state(state)
}

/// `GET /healthz` — liveness probe.
async fn healthz() -> &'static str {
    "ok"
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use tower::util::ServiceExt;

    fn mock_state() -> AppState {
        // Build a minimal AppState without a real DB for unit testing.
        // Routes that touch the DB will panic; only `/healthz` is tested here.
        use sqlx::postgres::PgPoolOptions;

        // We can't easily build a PgPool without a running DB in a unit test,
        // so we use a lazy pool that never connects.
        let pool = sqlx::PgPool::connect_lazy("postgres://localhost/kainetic_cloud_test")
            .expect("lazy pool creation never fails");

        let config = crate::config::CloudConfig::from_env();
        AppState::new(pool, config)
    }

    #[tokio::test]
    async fn healthz_returns_ok() {
        let app = build_router(mock_state());
        let resp = app
            .oneshot(Request::builder().uri("/healthz").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }
}
