//! `TelemetryError` — errors produced during telemetry initialisation.

use thiserror::Error;

/// Errors that can occur when initialising or configuring telemetry.
#[derive(Debug, Error)]
pub enum TelemetryError {
    /// A Prometheus metric could not be registered (e.g. duplicate name).
    #[error("Prometheus error: {0}")]
    Prometheus(String),

    /// The `OpenTelemetry` OTLP pipeline could not be initialised.
    ///
    /// Only produced when the `otlp` feature is enabled.
    #[error("OTel initialisation error: {0}")]
    OtelInit(String),
}
