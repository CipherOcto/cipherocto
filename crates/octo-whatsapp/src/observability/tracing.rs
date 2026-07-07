//! OTLP tracing exporter. Phase 5 Part B §Task 15.
//!
//! When the `otlp` feature is enabled, [`init_otlp`] wires a batch
//! exporter into the `tracing-opentelemetry` layer so all
//! `tracing::*!` calls flow out via OTLP gRPC.
//!
//! When the feature is DISABLED (the default), this module compiles
//! to a no-op stub. Library code paths that touch tracing use only
//! the local `tracing` crate, which is always available regardless
//! of the `otlp` feature flag.
//!
//! The OTLP feature is OFF by default per plan §A8 — operators
//! opt in via `cargo build --features otlp`.

/// Error type for OTLP initialization. Always available; the
/// feature-gated impls collapse to a single variant when `otlp` is
/// off.
#[derive(Debug, thiserror::Error)]
pub enum OtlpError {
    #[error("otlp feature disabled: rebuild with --features otlp")]
    FeatureDisabled,
    #[error("otlp init: {0}")]
    Init(String),
}

#[cfg(not(feature = "otlp"))]
pub fn init_otlp(_endpoint: &str, _service_name: &str) -> Result<(), OtlpError> {
    Err(OtlpError::FeatureDisabled)
}

#[cfg(feature = "otlp")]
pub fn init_otlp(endpoint: &str, service_name: &str) -> Result<(), OtlpError> {
    use opentelemetry::trace::TracerProvider as _;
    use opentelemetry::KeyValue;
    use opentelemetry_otlp::{SpanExporter, WithExportConfig};
    use opentelemetry_sdk::trace::TracerProvider;
    use opentelemetry_sdk::Resource;
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::util::SubscriberInitExt;

    let exporter = SpanExporter::builder()
        .with_tonic()
        .with_endpoint(endpoint)
        .build()
        .map_err(|e| OtlpError::Init(format!("exporter: {e}")))?;

    let provider = TracerProvider::builder()
        .with_batch_exporter(exporter, opentelemetry_sdk::runtime::Tokio)
        .with_resource(Resource::new(vec![KeyValue::new(
            "service.name",
            service_name.to_string(),
        )]))
        .build();
    let tracer = provider.tracer("octo-whatsapp");
    let otel_layer = tracing_opentelemetry::OpenTelemetryLayer::new(tracer);

    tracing_subscriber::registry()
        .with(otel_layer)
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .try_init()
        .map_err(|e| OtlpError::Init(format!("subscriber install: {e}")))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(not(feature = "otlp"))]
    #[test]
    fn init_otlp_returns_feature_disabled_when_feature_off() {
        let err = init_otlp("http://127.0.0.1:4317", "octo-whatsapp").unwrap_err();
        assert!(matches!(err, OtlpError::FeatureDisabled));
    }

    #[cfg(feature = "otlp")]
    #[tokio::test]
    async fn init_otlp_does_not_panic_with_random_endpoint() {
        // We don't want a panic on a wrong endpoint URL — the
        // exporter is async + lazy. Just confirm the call shape.
        // Actual export is a side-effect that requires a live
        // collector.
        let res = init_otlp("http://127.0.0.1:65535", "octo-whatsapp");
        // Either Ok (no panic) or a structured Err (collector not
        // reachable). The important invariant: no panic.
        let _ = res;
    }
}
