//! Prometheus metrics registry for Kainetic.

use prometheus::{
    CounterVec, GaugeVec, HistogramOpts, HistogramVec, Opts, Registry,
};

use crate::error::TelemetryError;

/// All Prometheus metrics registered by Kainetic.
///
/// Metrics are scoped to a custom [`Registry`] (not the default global one)
/// so that multiple runtimes in the same process — or isolated tests — each
/// maintain independent counters without collision.
///
/// Metric names follow the spec from SPEC §10.2:
/// - `kainetic_agent_runs_total{agent, status}`
/// - `kainetic_agent_run_duration_seconds{agent}`
/// - `kainetic_tool_calls_total{tool, status}`
/// - `kainetic_tool_duration_seconds{tool}`
/// - `kainetic_llm_requests_total{provider, model, status}`
/// - `kainetic_llm_request_duration_seconds{provider, model}`
/// - `kainetic_llm_tokens_total{provider, model, type}`
/// - `kainetic_llm_cost_usd_total{provider, model}`
/// - `kainetic_memory_ops_total{backend, operation}`
/// - `kainetic_active_runs{agent}`
pub struct KaineticMetrics {
    /// Total completed agent runs, labelled by agent name and final status.
    pub agent_runs_total: CounterVec,
    /// Histogram of agent run durations in seconds.
    pub agent_run_duration_seconds: HistogramVec,
    /// Total tool invocations, labelled by tool name and status.
    pub tool_calls_total: CounterVec,
    /// Histogram of tool call durations in seconds.
    pub tool_duration_seconds: HistogramVec,
    /// Total LLM requests, labelled by provider, model, and status.
    pub llm_requests_total: CounterVec,
    /// Histogram of LLM round-trip latency in seconds.
    pub llm_request_duration_seconds: HistogramVec,
    /// Total tokens consumed, labelled by provider, model, and type (`prompt`/`completion`).
    pub llm_tokens_total: CounterVec,
    /// Total estimated LLM cost in USD.
    pub llm_cost_usd_total: CounterVec,
    /// Total memory backend operations, labelled by backend type and operation name.
    pub memory_ops_total: CounterVec,
    /// Gauge of currently in-flight runs per agent.
    pub active_runs: GaugeVec,
}

/// Latency buckets in seconds — tuned for LLM workloads where P99 can be several seconds.
const LATENCY_BUCKETS_S: &[f64] = &[
    0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0, 30.0, 60.0,
];

impl KaineticMetrics {
    /// Creates and registers all metrics with the given `registry`.
    ///
    /// # Errors
    ///
    /// Returns [`TelemetryError::Prometheus`] if any metric fails to register
    /// (e.g. duplicate name — should never happen unless called twice on the
    /// same registry).
    pub fn new(registry: &Registry) -> Result<Self, TelemetryError> {
        macro_rules! counter {
            ($name:expr, $help:expr, $labels:expr) => {{
                let c = CounterVec::new(Opts::new($name, $help), $labels)
                    .map_err(|e| TelemetryError::Prometheus(e.to_string()))?;
                registry
                    .register(Box::new(c.clone()))
                    .map_err(|e| TelemetryError::Prometheus(e.to_string()))?;
                c
            }};
        }

        macro_rules! histogram {
            ($name:expr, $help:expr, $labels:expr) => {{
                let h = HistogramVec::new(
                    HistogramOpts::new($name, $help).buckets(LATENCY_BUCKETS_S.to_vec()),
                    $labels,
                )
                .map_err(|e| TelemetryError::Prometheus(e.to_string()))?;
                registry
                    .register(Box::new(h.clone()))
                    .map_err(|e| TelemetryError::Prometheus(e.to_string()))?;
                h
            }};
        }

        macro_rules! gauge {
            ($name:expr, $help:expr, $labels:expr) => {{
                let g = GaugeVec::new(Opts::new($name, $help), $labels)
                    .map_err(|e| TelemetryError::Prometheus(e.to_string()))?;
                registry
                    .register(Box::new(g.clone()))
                    .map_err(|e| TelemetryError::Prometheus(e.to_string()))?;
                g
            }};
        }

        Ok(Self {
            agent_runs_total: counter!(
                "kainetic_agent_runs_total",
                "Total completed agent runs.",
                &["agent", "status"]
            ),
            agent_run_duration_seconds: histogram!(
                "kainetic_agent_run_duration_seconds",
                "Agent run wall-clock duration in seconds.",
                &["agent"]
            ),
            tool_calls_total: counter!(
                "kainetic_tool_calls_total",
                "Total tool invocations.",
                &["tool", "status"]
            ),
            tool_duration_seconds: histogram!(
                "kainetic_tool_duration_seconds",
                "Tool call wall-clock duration in seconds.",
                &["tool"]
            ),
            llm_requests_total: counter!(
                "kainetic_llm_requests_total",
                "Total LLM completion requests.",
                &["provider", "model", "status"]
            ),
            llm_request_duration_seconds: histogram!(
                "kainetic_llm_request_duration_seconds",
                "LLM request round-trip duration in seconds.",
                &["provider", "model"]
            ),
            llm_tokens_total: counter!(
                "kainetic_llm_tokens_total",
                "Total tokens consumed (prompt + completion).",
                &["provider", "model", "type"]
            ),
            llm_cost_usd_total: counter!(
                "kainetic_llm_cost_usd_total",
                "Total estimated LLM cost in USD.",
                &["provider", "model"]
            ),
            memory_ops_total: counter!(
                "kainetic_memory_ops_total",
                "Total memory backend operations.",
                &["backend", "operation"]
            ),
            active_runs: gauge!(
                "kainetic_active_runs",
                "Currently in-flight agent runs.",
                &["agent"]
            ),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registers_without_error() {
        let registry = Registry::new();
        KaineticMetrics::new(&registry).unwrap();
    }

    #[test]
    fn duplicate_registry_errors() {
        let registry = Registry::new();
        KaineticMetrics::new(&registry).unwrap();
        // Second call on same registry must fail.
        assert!(KaineticMetrics::new(&registry).is_err());
    }

    #[test]
    fn counter_increments() {
        let registry = Registry::new();
        let m = KaineticMetrics::new(&registry).unwrap();
        m.agent_runs_total
            .with_label_values(&["my_agent", "success"])
            .inc();
        let families = registry.gather();
        let run_fam = families
            .iter()
            .find(|f| f.get_name() == "kainetic_agent_runs_total")
            .expect("metric family not found");
        let val = run_fam.get_metric()[0].get_counter().get_value();
        assert!((val - 1.0).abs() < f64::EPSILON);
    }
}
