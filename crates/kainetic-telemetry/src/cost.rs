//! `CostAccumulator` — per-run and per-hour cost tracking with configurable alerts.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use dashmap::DashMap;
use kainetic_schema::RunId;

/// An alert fired when a cost threshold is breached.
#[derive(Debug, Clone)]
pub enum CostAlert {
    /// A single run exceeded the per-run cost limit.
    PerRunExceeded {
        /// The run that breached the limit.
        run_id: RunId,
        /// The accumulated cost for this run in USD.
        cost_usd: f64,
        /// The configured threshold in USD.
        threshold_usd: f64,
    },
    /// Total spend in the rolling hour exceeded the hourly limit.
    PerHourExceeded {
        /// Total spend across all runs in the last hour in USD.
        total_usd: f64,
        /// The configured threshold in USD.
        threshold_usd: f64,
    },
}

/// Tracks cumulative cost per run and raises [`CostAlert`]s when configured
/// thresholds are breached.
///
/// Thread-safe — clone-able via [`Arc`] sharing.
#[derive(Clone)]
pub struct CostAccumulator {
    inner: Arc<Inner>,
}

struct Inner {
    /// Cumulative cost per active run.
    per_run: DashMap<RunId, f64>,
    /// Total spend this rolling hour, stored as `f64` bits in an `AtomicU64`.
    hourly_total_bits: AtomicU64,
    /// When the current one-hour window started.
    window_start: std::sync::Mutex<Instant>,
    alert_per_run_usd: Option<f64>,
    alert_per_hour_usd: Option<f64>,
}

impl CostAccumulator {
    /// Creates a new accumulator with optional alert thresholds.
    #[must_use]
    pub fn new(alert_per_run_usd: Option<f64>, alert_per_hour_usd: Option<f64>) -> Self {
        Self {
            inner: Arc::new(Inner {
                per_run: DashMap::new(),
                hourly_total_bits: AtomicU64::new(0),
                window_start: std::sync::Mutex::new(Instant::now()),
                alert_per_run_usd,
                alert_per_hour_usd,
            }),
        }
    }

    /// Adds `cost_usd` to the accumulator for `run_id`.
    ///
    /// Returns a list of [`CostAlert`]s triggered by this addition (may be
    /// empty, or contain one or two alerts).
    #[must_use]
    pub fn add(&self, run_id: RunId, cost_usd: f64) -> Vec<CostAlert> {
        let mut alerts = Vec::new();

        // Per-run accumulation.
        let new_run_cost = {
            let mut entry = self.inner.per_run.entry(run_id).or_insert(0.0);
            *entry += cost_usd;
            *entry
        };

        if let Some(limit) = self.inner.alert_per_run_usd {
            if new_run_cost > limit {
                alerts.push(CostAlert::PerRunExceeded {
                    run_id,
                    cost_usd: new_run_cost,
                    threshold_usd: limit,
                });
            }
        }

        // Hourly accumulation — roll the window if more than an hour has passed.
        self.maybe_roll_window();
        let new_hourly = self.add_to_hourly(cost_usd);

        if let Some(limit) = self.inner.alert_per_hour_usd {
            if new_hourly > limit {
                alerts.push(CostAlert::PerHourExceeded {
                    total_usd: new_hourly,
                    threshold_usd: limit,
                });
            }
        }

        alerts
    }

    /// Removes the accumulator entry for a completed run and returns its total cost.
    #[must_use]
    pub fn finish_run(&self, run_id: RunId) -> f64 {
        self.inner
            .per_run
            .remove(&run_id)
            .map_or(0.0, |(_, v)| v)
    }

    /// Returns the current accumulated cost for a run, or 0 if not tracked.
    #[must_use]
    pub fn run_cost(&self, run_id: RunId) -> f64 {
        self.inner
            .per_run
            .get(&run_id)
            .map_or(0.0, |v| *v)
    }

    /// Returns the total spend accumulated in the current rolling-hour window.
    #[must_use]
    pub fn hourly_total(&self) -> f64 {
        f64::from_bits(self.inner.hourly_total_bits.load(Ordering::Relaxed))
    }

    fn maybe_roll_window(&self) {
        let mut start = self.inner.window_start.lock().unwrap();
        if start.elapsed() >= Duration::from_secs(3600) {
            self.inner.hourly_total_bits.store(0, Ordering::Relaxed);
            *start = Instant::now();
        }
    }

    fn add_to_hourly(&self, cost_usd: f64) -> f64 {
        // CAS loop: fetch current bits, add, store.
        loop {
            let current_bits = self.inner.hourly_total_bits.load(Ordering::Relaxed);
            let current = f64::from_bits(current_bits);
            let new = current + cost_usd;
            let new_bits = new.to_bits();
            if self
                .inner
                .hourly_total_bits
                .compare_exchange_weak(current_bits, new_bits, Ordering::Relaxed, Ordering::Relaxed)
                .is_ok()
            {
                return new;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accumulates_per_run_cost() {
        let acc = CostAccumulator::new(None, None);
        let id = RunId::new();
        let _ = acc.add(id, 0.05);
        let _ = acc.add(id, 0.03);
        assert!((acc.run_cost(id) - 0.08).abs() < 1e-9);
    }

    #[test]
    fn finish_run_clears_entry() {
        let acc = CostAccumulator::new(None, None);
        let id = RunId::new();
        let _ = acc.add(id, 0.10);
        let total = acc.finish_run(id);
        assert!((total - 0.10).abs() < 1e-9);
        assert!((acc.run_cost(id)).abs() < 1e-9);
    }

    #[test]
    fn per_run_alert_fires_on_breach() {
        let acc = CostAccumulator::new(Some(0.05), None);
        let id = RunId::new();
        let alerts = acc.add(id, 0.06);
        assert_eq!(alerts.len(), 1);
        assert!(matches!(alerts[0], CostAlert::PerRunExceeded { .. }));
    }

    #[test]
    fn per_run_alert_does_not_fire_below_threshold() {
        let acc = CostAccumulator::new(Some(0.10), None);
        let id = RunId::new();
        let alerts = acc.add(id, 0.05);
        assert!(alerts.is_empty());
    }

    #[test]
    fn hourly_total_accumulates_across_runs() {
        let acc = CostAccumulator::new(None, None);
        let _ = acc.add(RunId::new(), 0.20);
        let _ = acc.add(RunId::new(), 0.30);
        assert!((acc.hourly_total() - 0.50).abs() < 1e-9);
    }

    #[test]
    fn hourly_alert_fires_on_breach() {
        let acc = CostAccumulator::new(None, Some(0.05));
        let alerts = acc.add(RunId::new(), 0.10);
        let has_hourly = alerts
            .iter()
            .any(|a| matches!(a, CostAlert::PerHourExceeded { .. }));
        assert!(has_hourly);
    }
}
