//! `TelemetryEventHandler` — bridges `AgentEvent` to Prometheus metrics and cost tracking.

use std::sync::Arc;
use std::time::Instant;

use dashmap::DashMap;
use kainetic_core::AgentEvent;
use kainetic_schema::RunId;
use tokio::sync::broadcast;
use tokio::task::JoinHandle;
use tracing::{debug, warn};

use crate::cost::{CostAccumulator, CostAlert};
use crate::metrics::KaineticMetrics;

/// State tracked for each in-flight run.
struct RunState {
    agent: String,
    started_at: Instant,
}

/// State tracked for each in-flight LLM call within a run.
struct LlmCallState {
    provider: String,
    model: String,
}

/// Subscribes to the [`AgentEvent`] broadcast channel and updates Prometheus
/// metrics and the [`CostAccumulator`] for every event.
///
/// Spawn via [`TelemetryEventHandler::spawn`], which returns a [`JoinHandle`]
/// you can abort to stop the handler.
pub struct TelemetryEventHandler {
    metrics: Arc<KaineticMetrics>,
    cost: CostAccumulator,
    /// Active runs: `run_id` → [`RunState`].
    runs: Arc<DashMap<RunId, RunState>>,
    /// Active LLM calls: `run_id` → [`LlmCallState`] (one per run at a time).
    llm_calls: Arc<DashMap<RunId, LlmCallState>>,
}

impl TelemetryEventHandler {
    /// Creates a new handler backed by the given metrics and accumulator.
    #[must_use]
    pub fn new(metrics: Arc<KaineticMetrics>, cost: CostAccumulator) -> Self {
        Self {
            metrics,
            cost,
            runs: Arc::new(DashMap::new()),
            llm_calls: Arc::new(DashMap::new()),
        }
    }

    /// Spawns a background tokio task that loops over events until the sender
    /// is dropped or the task is aborted.
    #[must_use]
    pub fn spawn(self, mut rx: broadcast::Receiver<AgentEvent>) -> JoinHandle<()> {
        tokio::spawn(async move {
            loop {
                match rx.recv().await {
                    Ok(event) => self.handle(&event),
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        warn!("TelemetryEventHandler lagged, skipped {n} events");
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
            debug!("TelemetryEventHandler exiting — event channel closed");
        })
    }

    fn handle(&self, event: &AgentEvent) {
        match event {
            AgentEvent::RunStarted { run_id, agent } => self.on_run_started(*run_id, agent),
            AgentEvent::LlmCallStarted {
                run_id,
                provider,
                model,
                ..
            } => self.on_llm_started(*run_id, provider, model),
            AgentEvent::LlmCallCompleted {
                run_id,
                prompt_tokens,
                completion_tokens,
                latency_ms,
            } => self.on_llm_completed(*run_id, *prompt_tokens, *completion_tokens, *latency_ms),
            AgentEvent::ToolCallCompleted {
                tool, latency_ms, ..
            } => self.on_tool_completed(tool, *latency_ms),
            AgentEvent::ToolCallFailed { tool, .. } => self.on_tool_failed(tool),
            AgentEvent::MemoryRead { hit, .. } => self.on_memory_read(*hit),
            AgentEvent::MemoryWrite { .. } => self.on_memory_write(),
            AgentEvent::RunCompleted {
                run_id,
                cost_usd,
                latency_ms,
                ..
            } => self.on_run_completed(*run_id, *cost_usd, *latency_ms),
            AgentEvent::RunFailed { run_id, .. } => self.on_run_failed(*run_id),
            // Non-exhaustive: ignore any future variants gracefully.
            _ => {}
        }
    }

    fn on_run_started(&self, run_id: RunId, agent: &str) {
        self.metrics.active_runs.with_label_values(&[agent]).inc();
        self.runs.insert(
            run_id,
            RunState {
                agent: agent.to_owned(),
                started_at: Instant::now(),
            },
        );
    }

    fn on_llm_started(&self, run_id: RunId, provider: &str, model: &str) {
        self.llm_calls.insert(
            run_id,
            LlmCallState {
                provider: provider.to_owned(),
                model: model.to_owned(),
            },
        );
    }

    #[allow(clippy::cast_precision_loss)]
    fn on_llm_completed(
        &self,
        run_id: RunId,
        prompt_tokens: u32,
        completion_tokens: u32,
        latency_ms: u64,
    ) {
        if let Some((_, state)) = self.llm_calls.remove(&run_id) {
            let p = state.provider.as_str();
            let m = state.model.as_str();
            self.metrics
                .llm_requests_total
                .with_label_values(&[p, m, "success"])
                .inc();
            self.metrics
                .llm_request_duration_seconds
                .with_label_values(&[p, m])
                .observe(latency_ms as f64 / 1000.0);
            self.metrics
                .llm_tokens_total
                .with_label_values(&[p, m, "prompt"])
                .inc_by(f64::from(prompt_tokens));
            self.metrics
                .llm_tokens_total
                .with_label_values(&[p, m, "completion"])
                .inc_by(f64::from(completion_tokens));
        }
    }

    #[allow(clippy::cast_precision_loss)]
    fn on_tool_completed(&self, tool: &str, latency_ms: u64) {
        self.metrics
            .tool_calls_total
            .with_label_values(&[tool, "success"])
            .inc();
        self.metrics
            .tool_duration_seconds
            .with_label_values(&[tool])
            .observe(latency_ms as f64 / 1000.0);
    }

    fn on_tool_failed(&self, tool: &str) {
        self.metrics
            .tool_calls_total
            .with_label_values(&[tool, "failed"])
            .inc();
    }

    fn on_memory_read(&self, hit: bool) {
        let op = if hit { "read_hit" } else { "read_miss" };
        self.metrics
            .memory_ops_total
            .with_label_values(&["unknown", op])
            .inc();
    }

    fn on_memory_write(&self) {
        self.metrics
            .memory_ops_total
            .with_label_values(&["unknown", "write"])
            .inc();
    }

    #[allow(clippy::cast_precision_loss)]
    fn on_run_completed(&self, run_id: RunId, cost_usd: f64, latency_ms: u64) {
        let agent = self.finish_run(run_id, "success", latency_ms as f64 / 1000.0);

        self.metrics
            .llm_cost_usd_total
            .with_label_values(&["unknown", "unknown"])
            .inc_by(cost_usd);

        let cost_alerts = self.cost.add(run_id, cost_usd);
        let _ = self.cost.finish_run(run_id);

        for alert in &cost_alerts {
            Self::log_cost_alert(&agent, alert);
        }
    }

    fn on_run_failed(&self, run_id: RunId) {
        // Clear any dangling LLM call state.
        if let Some((_, state)) = self.llm_calls.remove(&run_id) {
            self.metrics
                .llm_requests_total
                .with_label_values(&[&state.provider, &state.model, "failed"])
                .inc();
        }
        self.finish_run(run_id, "failed", 0.0);
    }

    /// Finalises a run: removes from in-flight map, decrements gauge, records
    /// duration + total counter. Returns agent name (or `"unknown"`).
    fn finish_run(&self, run_id: RunId, status: &str, duration_secs: f64) -> String {
        if let Some((_, state)) = self.runs.remove(&run_id) {
            let agent = state.agent.as_str();
            let secs = if duration_secs > 0.0 {
                duration_secs
            } else {
                state.started_at.elapsed().as_secs_f64()
            };
            self.metrics.active_runs.with_label_values(&[agent]).dec();
            self.metrics
                .agent_runs_total
                .with_label_values(&[agent, status])
                .inc();
            self.metrics
                .agent_run_duration_seconds
                .with_label_values(&[agent])
                .observe(secs);
            state.agent
        } else {
            "unknown".to_owned()
        }
    }

    fn log_cost_alert(agent: &str, alert: &CostAlert) {
        match alert {
            CostAlert::PerRunExceeded {
                run_id,
                cost_usd,
                threshold_usd,
            } => {
                warn!(
                    agent,
                    run_id = %run_id,
                    cost_usd,
                    threshold_usd,
                    "COST ALERT: run exceeded per-run cost threshold"
                );
            }
            CostAlert::PerHourExceeded {
                total_usd,
                threshold_usd,
            } => {
                warn!(
                    agent,
                    total_usd, threshold_usd, "COST ALERT: hourly spend exceeded threshold"
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use kainetic_schema::RunId;
    use prometheus::Registry;
    use tokio::sync::broadcast;

    use super::*;
    use crate::metrics::KaineticMetrics;

    fn make_handler() -> (TelemetryEventHandler, Arc<KaineticMetrics>, Registry) {
        let registry = Registry::new();
        let metrics = Arc::new(KaineticMetrics::new(&registry).unwrap());
        let cost = CostAccumulator::new(None, None);
        let handler = TelemetryEventHandler::new(Arc::clone(&metrics), cost);
        (handler, metrics, registry)
    }

    #[test]
    fn run_started_increments_active_runs() {
        let (handler, metrics, _) = make_handler();
        let id = RunId::new();
        handler.handle(&AgentEvent::RunStarted {
            run_id: id,
            agent: "test_agent".to_owned(),
        });
        let val = metrics.active_runs.with_label_values(&["test_agent"]).get();
        assert!((val - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn run_completed_decrements_active_runs_and_records_counter() {
        let (handler, metrics, _) = make_handler();
        let id = RunId::new();
        handler.handle(&AgentEvent::RunStarted {
            run_id: id,
            agent: "my_agent".to_owned(),
        });
        handler.handle(&AgentEvent::RunCompleted {
            run_id: id,
            total_tokens: 100,
            cost_usd: 0.01,
            latency_ms: 500,
        });
        assert!((metrics.active_runs.with_label_values(&["my_agent"]).get()).abs() < f64::EPSILON);
        assert!(
            (metrics
                .agent_runs_total
                .with_label_values(&["my_agent", "success"])
                .get()
                - 1.0)
                .abs()
                < f64::EPSILON
        );
    }

    #[test]
    fn tool_completed_increments_tool_counter() {
        let (handler, metrics, _) = make_handler();
        let id = RunId::new();
        handler.handle(&AgentEvent::ToolCallCompleted {
            run_id: id,
            tool: "web_search".to_owned(),
            output: serde_json::json!({}),
            latency_ms: 100,
        });
        let val = metrics
            .tool_calls_total
            .with_label_values(&["web_search", "success"])
            .get();
        assert!((val - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn tool_failed_increments_failed_counter() {
        let (handler, metrics, _) = make_handler();
        let id = RunId::new();
        handler.handle(&AgentEvent::ToolCallFailed {
            run_id: id,
            tool: "bad_tool".to_owned(),
            error: "oh no".to_owned(),
        });
        let val = metrics
            .tool_calls_total
            .with_label_values(&["bad_tool", "failed"])
            .get();
        assert!((val - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn llm_tokens_are_recorded() {
        let (handler, metrics, _) = make_handler();
        let id = RunId::new();
        handler.handle(&AgentEvent::LlmCallStarted {
            run_id: id,
            provider: "anthropic".to_owned(),
            model: "claude-3-5-sonnet".to_owned(),
            messages: 1,
        });
        handler.handle(&AgentEvent::LlmCallCompleted {
            run_id: id,
            prompt_tokens: 50,
            completion_tokens: 20,
            latency_ms: 300,
        });
        let prompt_val = metrics
            .llm_tokens_total
            .with_label_values(&["anthropic", "claude-3-5-sonnet", "prompt"])
            .get();
        assert!((prompt_val - 50.0).abs() < f64::EPSILON);
        let comp_val = metrics
            .llm_tokens_total
            .with_label_values(&["anthropic", "claude-3-5-sonnet", "completion"])
            .get();
        assert!((comp_val - 20.0).abs() < f64::EPSILON);
    }

    #[tokio::test]
    async fn spawn_processes_events_from_channel() {
        let registry = Registry::new();
        let metrics = Arc::new(KaineticMetrics::new(&registry).unwrap());
        let cost = CostAccumulator::new(None, None);
        let handler = TelemetryEventHandler::new(Arc::clone(&metrics), cost);

        let (tx, rx) = broadcast::channel(64);
        let handle = handler.spawn(rx);

        let id = RunId::new();
        tx.send(AgentEvent::RunStarted {
            run_id: id,
            agent: "async_agent".to_owned(),
        })
        .unwrap();

        // Give the background task a moment to process.
        tokio::time::sleep(Duration::from_millis(10)).await;

        let val = metrics
            .active_runs
            .with_label_values(&["async_agent"])
            .get();
        assert!((val - 1.0).abs() < f64::EPSILON);

        handle.abort();
    }
}
