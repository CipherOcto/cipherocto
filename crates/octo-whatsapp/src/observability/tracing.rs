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

/// Phase 7.J.2: install a `tracing_subscriber::fmt` layer writing to
/// stderr, with an `EnvFilter` read from `RUST_LOG` (defaulting to
/// `info`). This is the lightest-possible tracing wiring: no OTLP
/// collector, no file rotation, just a stderr line per event.
///
/// Why this exists: the default `cargo build -p octo-whatsapp` does
/// NOT install a global subscriber (the `otlp` feature is opt-in and
/// its `init_otlp()` is a no-op stub — see `init_otlp` above). That
/// means every `tracing::*!` call from the daemon, wacore, octo_cable,
/// etc. silently goes to /dev/null — the daemon log file is always
/// empty during normal operation, regardless of failure mode. This
/// helper fixes that for the in-tree `octo-whatsapp` binary without
/// requiring an OTLP deployment.
///
/// Idempotent: returns Ok(()) silently if a subscriber is already set
/// (the typical case when both `tracing-stdout` and `otlp` are on).
///
/// When the `tracing-stdout` cargo feature is OFF this is a no-op —
/// callers can invoke it unconditionally from the daemon dispatch
/// path without `#[cfg]` gates on every callsite.
pub fn init_tracing_stdout() {
    #[cfg(feature = "tracing-stdout")]
    {
        use tracing_subscriber::layer::SubscriberExt;
        use tracing_subscriber::util::SubscriberInitExt;

        let filter = tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));

        // `.with_writer(std::io::stderr)` keeps stdout clean for MCP
        // stdio relay (the MCP server reads stdin/stdout — anything we
        // write to stdout corrupts the JSON-RPC stream).
        let layer = tracing_subscriber::fmt::layer()
            .with_writer(std::io::stderr)
            .with_target(true)
            .with_thread_ids(false)
            .with_line_number(false);

        // `try_init` returns Err if a subscriber is already set. That's
        // fine — the otlp path may have already installed one, or a
        // previous test in the same process beat us to it.
        let _ = tracing_subscriber::registry()
            .with(filter)
            .with(layer)
            .try_init();
    }
    #[cfg(not(feature = "tracing-stdout"))]
    {
        // Intentionally a no-op. The default `cargo build` does NOT
        // install any subscriber; every `tracing::*!` in daemon /
        // wacore / octo_cable silently goes nowhere. Operators who
        // want visibility build with `--features tracing-stdout` (or
        // `--features otlp`).
    }
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

    /// Phase 7.J.2: `init_tracing_stdout()` is safe to call from any
    /// build configuration. When the `tracing-stdout` feature is OFF,
    /// it's a compile-time no-op (per the `#[cfg(not(...))]` arms above).
    /// When the feature is ON, it must (a) not panic on first call,
    /// (b) silently no-op on second call (idempotent — `try_init`
    /// returns Err but we deliberately discard it).
    #[test]
    fn init_tracing_stdout_is_idempotent_and_does_not_panic() {
        // If feature is off, this is a no-op (verifying "no panic" is
        // enough — the build itself proves the cfg arms compile).
        init_tracing_stdout();
        init_tracing_stdout(); // second call: also no-op / silently ignored

        // If we're here without panicking, the invariant holds.
        // The actual subscriber side-effect only happens when
        // `tracing-stdout` is on — that path is exercised by the
        // live CLI integration test in `tests/live_daemon_test.rs`.
    }
}
