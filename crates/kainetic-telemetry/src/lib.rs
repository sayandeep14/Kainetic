//! Observability for Kainetic: `OpenTelemetry` tracing, Prometheus metrics, and cost tracking.
//!
//! # Quick Start
//!
//! ```rust,ignore
//! use kainetic_core::KaineticRuntime;
//! use kainetic_providers::AnthropicProvider;
//! use kainetic_telemetry::TelemetryConfig;
//!
//! let runtime = KaineticRuntime::builder()
//!     .provider(AnthropicProvider::from_env()?)
//!     .build();
//!
//! // One line to enable full observability.
//! let _tel = TelemetryConfig::builder()
//!     .service_name("my-service")
//!     .metrics_port(9090)
//!     .build()
//!     .attach(runtime.subscribe_events())?;
//!
//! // runtime.run(...) — all events are now automatically measured.
//! ```
//!
//! ## OTLP Export
//!
//! Enable the `otlp` feature and set an endpoint to export traces to any
//! OpenTelemetry-compatible backend (Grafana Tempo, Jaeger, Honeycomb, …):
//!
//! ```toml
//! [dependencies]
//! kainetic-telemetry = { version = "*", features = ["otlp"] }
//! ```
//!
//! ```rust,no_run
//! # use kainetic_telemetry::TelemetryConfig;
//! # use tokio::sync::broadcast;
//! # let (_, rx) = broadcast::channel(64);
//! let _tel = TelemetryConfig::builder()
//!     .otlp_endpoint("http://localhost:4317")
//!     .build()
//!     .attach(rx)
//!     .unwrap();
//! ```
//!
//! ## Cost Alerts
//!
//! ```rust,no_run
//! # use kainetic_telemetry::TelemetryConfig;
//! # use tokio::sync::broadcast;
//! # let (_, rx) = broadcast::channel(64);
//! let _tel = TelemetryConfig::builder()
//!     .alert_cost_per_run_usd(0.10)   // warn if a single run > $0.10
//!     .alert_cost_per_hour_usd(100.0) // warn if hourly spend > $100
//!     .build()
//!     .attach(rx)
//!     .unwrap();
//! ```
#![deny(clippy::all, clippy::pedantic, missing_docs, unsafe_code)]

mod config;
mod cost;
mod error;
mod event_handler;
mod metrics;
mod otel;
mod server;

pub use config::{TelemetryConfig, TelemetryHandle};
pub use cost::{CostAccumulator, CostAlert};
pub use error::TelemetryError;
pub use event_handler::TelemetryEventHandler;
pub use metrics::KaineticMetrics;
pub use otel::init_fmt_tracing;
pub use server::start_metrics_server;

#[cfg(feature = "otlp")]
pub use otel::{init_tracing, OtelGuard};
