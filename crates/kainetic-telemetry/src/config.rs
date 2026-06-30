//! `TelemetryConfig` — builder for the full observability stack.

use std::sync::Arc;

use kainetic_core::AgentEvent;
use prometheus::Registry;
use tokio::sync::broadcast;
use tokio::task::JoinHandle;

use crate::cost::CostAccumulator;
use crate::error::TelemetryError;
use crate::event_handler::TelemetryEventHandler;
use crate::metrics::KaineticMetrics;
use crate::server::start_metrics_server;

/// Configuration for the Kainetic observability stack.
///
/// # Example
///
/// ```rust,ignore
/// use kainetic_telemetry::TelemetryConfig;
/// use kainetic_core::KaineticRuntime;
/// use kainetic_providers::AnthropicProvider;
///
/// let runtime = KaineticRuntime::builder()
///     .provider(AnthropicProvider::from_env()?)
///     .build();
///
/// let _handle = TelemetryConfig::builder()
///     .service_name("my-agent")
///     .metrics_port(9090)
///     .alert_cost_per_run_usd(0.10)
///     .build()
///     .attach(runtime.subscribe_events())?;
/// ```
#[derive(Default)]
pub struct TelemetryConfig {
    service_name: Option<String>,
    otlp_endpoint: Option<String>,
    metrics_port: Option<u16>,
    alert_cost_per_run_usd: Option<f64>,
    alert_cost_per_hour_usd: Option<f64>,
}

impl TelemetryConfig {
    /// Returns a new builder.
    #[must_use]
    pub fn builder() -> Self {
        Self::default()
    }

    /// Sets the service name used in `OTel` trace attributes.
    ///
    /// Defaults to `"kainetic"`.
    #[must_use]
    pub fn service_name(mut self, name: impl Into<String>) -> Self {
        self.service_name = Some(name.into());
        self
    }

    /// Sets the OTLP endpoint for trace export (requires feature `otlp`).
    ///
    /// Example: `"http://localhost:4317"`.
    #[must_use]
    pub fn otlp_endpoint(mut self, endpoint: impl Into<String>) -> Self {
        self.otlp_endpoint = Some(endpoint.into());
        self
    }

    /// Starts a Prometheus `/metrics` HTTP server on the given port.
    ///
    /// If not set, no metrics server is started (metrics are still collected).
    #[must_use]
    pub fn metrics_port(mut self, port: u16) -> Self {
        self.metrics_port = Some(port);
        self
    }

    /// Fires a [`crate::CostAlert::PerRunExceeded`] warning when a single run
    /// exceeds this cost in USD.
    #[must_use]
    pub fn alert_cost_per_run_usd(mut self, usd: f64) -> Self {
        self.alert_cost_per_run_usd = Some(usd);
        self
    }

    /// Fires a [`crate::CostAlert::PerHourExceeded`] warning when total spend in the
    /// rolling hour exceeds this cost in USD.
    #[must_use]
    pub fn alert_cost_per_hour_usd(mut self, usd: f64) -> Self {
        self.alert_cost_per_hour_usd = Some(usd);
        self
    }

    /// Finalises configuration.
    #[must_use]
    pub fn build(self) -> Self {
        self
    }

    /// Wires the telemetry stack to the given `AgentEvent` receiver.
    ///
    /// This method:
    /// 1. Initialises tracing (fmt layer, plus OTLP if `otlp` feature + endpoint set).
    /// 2. Creates a Prometheus registry and registers all Kainetic metrics.
    /// 3. Spawns the [`TelemetryEventHandler`] background task.
    /// 4. Optionally starts the `/metrics` HTTP server.
    ///
    /// Returns a [`TelemetryHandle`] that keeps the background tasks alive.
    /// Drop the handle to stop all telemetry tasks.
    ///
    /// # Errors
    ///
    /// Returns [`TelemetryError`] if Prometheus metric registration fails or
    /// if the OTLP pipeline cannot be built.
    pub fn attach(
        self,
        rx: broadcast::Receiver<AgentEvent>,
    ) -> Result<TelemetryHandle, TelemetryError> {
        #[allow(unused_variables)]
        let service_name = self.service_name.unwrap_or_else(|| "kainetic".to_owned());

        // --- Tracing ---
        #[cfg(feature = "otlp")]
        let otel_guard = if let Some(ref endpoint) = self.otlp_endpoint {
            Some(crate::otel::init_tracing(endpoint, &service_name)?)
        } else {
            crate::otel::init_fmt_tracing();
            None
        };

        #[cfg(not(feature = "otlp"))]
        {
            if self.otlp_endpoint.is_some() {
                tracing::warn!(
                    "otlp_endpoint set but the `otlp` feature is not enabled — \
                     enable `kainetic-telemetry/otlp` to export traces"
                );
            }
            crate::otel::init_fmt_tracing();
        }

        // --- Prometheus ---
        let registry = Arc::new(Registry::new());
        let metrics = Arc::new(KaineticMetrics::new(&registry)?);

        // --- Event handler ---
        let cost = CostAccumulator::new(self.alert_cost_per_run_usd, self.alert_cost_per_hour_usd);
        let handler = TelemetryEventHandler::new(Arc::clone(&metrics), cost);
        let handler_handle = handler.spawn(rx);

        // --- Metrics server ---
        let server_handle = self
            .metrics_port
            .map(|port| start_metrics_server(port, Arc::clone(&registry)));

        Ok(TelemetryHandle {
            registry,
            metrics,
            handler_handle,
            server_handle,
            #[cfg(feature = "otlp")]
            _otel_guard: otel_guard,
        })
    }
}

/// Keeps the telemetry background tasks alive.
///
/// Dropping this handle aborts the event handler task and (if started) the
/// metrics server. Any buffered `OTel` spans are flushed on drop via
/// `OtelGuard`.
pub struct TelemetryHandle {
    /// The Prometheus registry — accessible for custom metrics or testing.
    pub registry: Arc<Registry>,
    /// All Kainetic metrics — accessible for inspection in tests.
    pub metrics: Arc<KaineticMetrics>,
    handler_handle: JoinHandle<()>,
    server_handle: Option<JoinHandle<()>>,
    #[cfg(feature = "otlp")]
    _otel_guard: Option<crate::otel::OtelGuard>,
}

impl Drop for TelemetryHandle {
    fn drop(&mut self) {
        self.handler_handle.abort();
        if let Some(ref h) = self.server_handle {
            h.abort();
        }
    }
}

#[cfg(test)]
mod tests {
    use kainetic_core::AgentEvent;
    use kainetic_schema::RunId;
    use tokio::sync::broadcast;

    use super::*;

    #[tokio::test]
    async fn attach_without_server_succeeds() {
        let (tx, rx) = broadcast::channel::<AgentEvent>(64);
        let handle = TelemetryConfig::builder()
            .service_name("test")
            .build()
            .attach(rx)
            .unwrap();

        let id = RunId::new();
        tx.send(AgentEvent::RunStarted {
            run_id: id,
            agent: "cfg_agent".to_owned(),
        })
        .unwrap();

        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        let val = handle
            .metrics
            .active_runs
            .with_label_values(&["cfg_agent"])
            .get();
        assert!((val - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn build_is_chainable() {
        let _ = TelemetryConfig::builder()
            .service_name("svc")
            .metrics_port(9091)
            .alert_cost_per_run_usd(0.05)
            .alert_cost_per_hour_usd(10.0)
            .build();
    }
}
