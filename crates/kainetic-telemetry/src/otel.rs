//! `OpenTelemetry` tracing setup — only compiled when feature `otlp` is enabled.

#[cfg(feature = "otlp")]
mod inner {
    use opentelemetry::trace::TracerProvider as _;
    use opentelemetry_otlp::WithExportConfig;
    use opentelemetry_sdk::{
        runtime::Tokio,
        trace::{Config, Sampler, TracerProvider},
    };
    use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

    use crate::error::TelemetryError;

    /// Returned by [`init_tracing`]; shuts down the OTel provider on drop.
    ///
    /// Hold this value alive for the entire application lifetime. When it
    /// drops, any buffered spans are flushed to the OTLP endpoint.
    pub struct OtelGuard {
        provider: TracerProvider,
    }

    impl Drop for OtelGuard {
        fn drop(&mut self) {
            if let Err(e) = self.provider.shutdown() {
                eprintln!("Failed to shut down OTel provider: {e}");
            }
        }
    }

    /// Initialises the global `tracing` subscriber with:
    ///
    /// - A `fmt` layer writing human-readable output to stderr (respects
    ///   `RUST_LOG`).
    /// - An `opentelemetry` layer that exports spans to `otlp_endpoint` via
    ///   OTLP/HTTP protobuf.
    ///
    /// # Errors
    ///
    /// Returns [`TelemetryError`] if the OTLP pipeline cannot be built or if
    /// a global subscriber is already installed.
    pub fn init_tracing(
        otlp_endpoint: &str,
        service_name: &str,
    ) -> Result<OtelGuard, TelemetryError> {
        let exporter = opentelemetry_otlp::new_exporter()
            .http()
            .with_endpoint(otlp_endpoint)
            .build_span_exporter()
            .map_err(|e| TelemetryError::OtelInit(e.to_string()))?;

        let provider = opentelemetry_sdk::trace::TracerProvider::builder()
            .with_batch_exporter(exporter, Tokio)
            .with_config(
                Config::default()
                    .with_sampler(Sampler::AlwaysOn)
                    .with_resource(opentelemetry_sdk::Resource::new(vec![
                        opentelemetry::KeyValue::new(
                            opentelemetry_semantic_conventions::resource::SERVICE_NAME,
                            service_name.to_owned(),
                        ),
                    ])),
            )
            .build();

        let tracer = provider.tracer(service_name.to_owned());
        let otel_layer = tracing_opentelemetry::layer().with_tracer(tracer);

        tracing_subscriber::registry()
            .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
            .with(tracing_subscriber::fmt::layer().with_writer(std::io::stderr))
            .with(otel_layer)
            .try_init()
            .map_err(|e| TelemetryError::OtelInit(e.to_string()))?;

        Ok(OtelGuard { provider })
    }
}

/// Initialises a `tracing` subscriber without `OTel` export (fmt layer only).
///
/// Respects `RUST_LOG`. Safe to call multiple times — subsequent calls are
/// no-ops if a subscriber is already installed.
pub fn init_fmt_tracing() {
    use tracing_subscriber::EnvFilter;
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .try_init();
}

#[cfg(feature = "otlp")]
pub use inner::{init_tracing, OtelGuard};
