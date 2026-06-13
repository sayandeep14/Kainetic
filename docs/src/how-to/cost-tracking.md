# Cost Tracking & Alerts

Kainetic tracks token usage and estimated USD cost for every run via `CostAccumulator` in `kainetic-telemetry`.

## Enabling cost tracking

```rust
use kainetic_telemetry::TelemetryConfig;

TelemetryConfig::prometheus(9090)
    .cost_alert_per_run_usd(0.50)       // alert if one run costs > $0.50
    .cost_alert_hourly_usd(10.00)       // alert if hourly spend > $10.00
    .on_alert(|alert| {
        eprintln!("Cost alert! {}: ${:.4}", alert.kind, alert.amount_usd);
        // Send to Slack, PagerDuty, etc.
    })
    .attach(runtime.subscribe_events())
    .await?;
```

## Reading cost from events

Subscribe to `AgentEvent::RunCompleted` to get per-run cost:

```rust
let mut rx = runtime.subscribe_events();
tokio::spawn(async move {
    while let Ok(event) = rx.recv().await {
        if let AgentEvent::RunCompleted { run_id, cost_usd, .. } = event {
            println!("Run {run_id} cost: ${cost_usd:.6}");
        }
    }
});
```

## Cost accuracy

Costs are estimated using hard-coded per-token prices. Actual invoices may differ:
- Prices are periodically updated; check the crate source for current rates.
- The estimates do not include image inputs or tool result tokens for some providers.
- Cache hits (prompt caching) are not reflected in the estimate.

For billing purposes, always use your provider's invoice as the source of truth.

## Prometheus metric

```
# HELP kainetic_cost_usd_total Total estimated cost in USD
# TYPE kainetic_cost_usd_total counter
kainetic_cost_usd_total{agent="researcher",model="claude-sonnet-4-6"} 0.0423
```
