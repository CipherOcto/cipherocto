//! Observability surface: Prometheus metrics, HTTP health/ready/metrics
//! server, and (optional) OTLP tracing export.
//!
//! Phase 5 Part B of `docs/plans/2026-07-07-whatsapp-runtime-cli-mcp-phase5.md`
//! ships:
//! - [`metrics`] — `Metrics` struct with the 14 named
//!   counters/gauges/histograms called out in §Observability.
//! - [`health_server`] — axum-based HTTP server with
//!   `/health`, `/ready`, `/metrics` (always bearer-protected).
//! - [`tracing`] — no-op stub when the `otlp` feature is OFF; an
//!   `init_otlp(endpoint, service_name)` helper that wires
//!   `tracing-opentelemetry` into the global subscriber when ON.
//!
//! All 14 metrics use HMAC-hashed 8-hex-char labels (`hash_label`) to
//! bound Prometheus cardinality. The hash secret is a
//! [`crate::config::ObservabilityConfig`] field.

#![deny(unsafe_code)]

pub mod health_server;
pub mod metrics;
pub mod tracing;

pub use health_server::{run_health_server, HealthServerHandle, METRICS_BEARER_ENV};
pub use metrics::{hash_label, Metrics, MetricsError};
// Phase 7.J.2: lightweight stderr tracing — re-exported so
// `cli::dispatch` can call it unconditionally (it's a no-op when the
// feature is off).
#[cfg(feature = "tracing-stdout")]
pub use tracing::init_tracing_stdout;
