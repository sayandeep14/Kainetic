//! Axum HTTP server that exposes Prometheus metrics at `GET /metrics`.

use std::sync::Arc;

use axum::{extract::State, http::StatusCode, response::IntoResponse, routing::get, Router};
use prometheus::{Encoder, Registry, TextEncoder};
use tokio::task::JoinHandle;

/// Starts a Prometheus metrics server on `0.0.0.0:{port}`.
///
/// Returns a [`JoinHandle`] for the server task. Abort it to stop the server.
///
/// The `/metrics` endpoint encodes the full `registry` in the standard
/// Prometheus text exposition format (`text/plain; version=0.0.4`).
///
/// # Panics
///
/// Panics if the TCP listener cannot be bound (e.g. port already in use).
#[must_use]
pub fn start_metrics_server(port: u16, registry: Arc<Registry>) -> JoinHandle<()> {
    tokio::spawn(async move {
        let router = Router::new()
            .route("/metrics", get(metrics_handler))
            .with_state(registry);

        let addr = format!("0.0.0.0:{port}");
        let listener = tokio::net::TcpListener::bind(&addr)
            .await
            .unwrap_or_else(|e| panic!("Failed to bind metrics server on {addr}: {e}"));

        tracing::info!(port, "Prometheus metrics server listening");

        axum::serve(listener, router)
            .await
            .unwrap_or_else(|e| tracing::error!("Metrics server error: {e}"));
    })
}

async fn metrics_handler(State(registry): State<Arc<Registry>>) -> impl IntoResponse {
    let encoder = TextEncoder::new();
    let families = registry.gather();
    let mut buf = Vec::new();
    if let Err(e) = encoder.encode(&families, &mut buf) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to encode metrics: {e}"),
        )
            .into_response();
    }
    (
        [(axum::http::header::CONTENT_TYPE, encoder.format_type())],
        buf,
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn metrics_endpoint_returns_text_plain() {
        use prometheus::Counter;

        // Bind on port 0 so the OS picks a free port.
        let registry = Arc::new(Registry::new());
        let counter = Counter::new("test_counter", "A test counter").unwrap();
        registry.register(Box::new(counter.clone())).unwrap();
        counter.inc();

        let router = Router::new()
            .route("/metrics", get(metrics_handler))
            .with_state(Arc::clone(&registry));

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });

        // Give the server a moment to start.
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;

        let resp = reqwest::get(format!("http://{addr}/metrics"))
            .await
            .unwrap();

        assert_eq!(resp.status(), 200);
        let body = resp.text().await.unwrap();
        assert!(body.contains("test_counter"), "body: {body}");
    }
}
