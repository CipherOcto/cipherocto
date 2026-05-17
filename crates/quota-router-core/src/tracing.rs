//! OpenTelemetry tracing module (RFC-0905).
//!
//! Provides OTLP export, W3C trace context propagation, and configurable sampling.

use opentelemetry::global;
use opentelemetry::trace::Tracer;
use opentelemetry::KeyValue;
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::trace::{self, Sampler, TracerProvider};
use opentelemetry_sdk::Resource;
use serde::{Deserialize, Serialize};

// ============================================================================
// TracingConfig
// ============================================================================

/// Configuration for OpenTelemetry tracing (RFC-0905 §2).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TracingConfig {
    /// OTLP endpoint (e.g., "http://localhost:4317").
    #[serde(default = "default_otlp_endpoint")]
    pub otlp_endpoint: String,

    /// Sampling rate (0.0 = none, 1.0 = all). Default: 1.0.
    #[serde(default = "default_sampling_rate")]
    pub sampling_rate: f64,

    /// Exporter type: "otlp" or "stdout". Default: "otlp".
    #[serde(default = "default_exporter")]
    pub exporter: String,

    /// Context propagation: "w3c" or "b3". Default: "w3c".
    #[serde(default = "default_propagation")]
    pub propagation: String,

    /// Service name for resource attributes.
    #[serde(default = "default_service_name")]
    pub service_name: String,

    /// Deployment environment (e.g., "production", "staging").
    #[serde(default)]
    pub deployment_environment: Option<String>,
}

fn default_otlp_endpoint() -> String {
    "http://localhost:4317".to_string()
}

fn default_sampling_rate() -> f64 {
    1.0
}

fn default_exporter() -> String {
    "otlp".to_string()
}

fn default_propagation() -> String {
    "w3c".to_string()
}

fn default_service_name() -> String {
    "quota-router".to_string()
}

impl Default for TracingConfig {
    fn default() -> Self {
        Self {
            otlp_endpoint: default_otlp_endpoint(),
            sampling_rate: default_sampling_rate(),
            exporter: default_exporter(),
            propagation: default_propagation(),
            service_name: default_service_name(),
            deployment_environment: None,
        }
    }
}

// ============================================================================
// Tracer Initialization
// ============================================================================

/// Initialize OpenTelemetry tracer with OTLP export.
///
/// Uses `install_batch()` for production (non-blocking span export).
/// Falls back to `install_simple()` for development/testing.
pub fn init_tracer(config: &TracingConfig) -> Result<(), opentelemetry::trace::TraceError> {
    let mut resource_kvs = vec![
        KeyValue::new("service.name", config.service_name.clone()),
        KeyValue::new("service.version", env!("CARGO_PKG_VERSION").to_string()),
    ];

    if let Some(ref env) = config.deployment_environment {
        resource_kvs.push(KeyValue::new("deployment.environment", env.clone()));
    }

    let resource = Resource::new(resource_kvs);

    let sampler = if config.sampling_rate >= 1.0 {
        Sampler::AlwaysOn
    } else if config.sampling_rate <= 0.0 {
        Sampler::AlwaysOff
    } else {
        Sampler::TraceIdRatioBased(config.sampling_rate)
    };

    let trace_config = trace::config()
        .with_resource(resource)
        .with_sampler(Sampler::ParentBased(Box::new(sampler)));

    match config.exporter.as_str() {
        "stdout" => {
            // For stdout, use no-op provider (tracing crate handles stdout)
            // In production, use OTLP exporter
            let provider = TracerProvider::builder().with_config(trace_config).build();
            global::set_tracer_provider(provider);
        }
        _ => {
            // OTLP exporter
            let exporter = opentelemetry_otlp::SpanExporter::builder()
                .with_tonic()
                .with_endpoint(&config.otlp_endpoint)
                .build()
                .map_err(|e| opentelemetry::trace::TraceError::from(e.to_string()))?;

            let provider = TracerProvider::builder()
                .with_config(trace_config)
                .with_batch_exporter(exporter, opentelemetry_sdk::runtime::Tokio)
                .build();
            global::set_tracer_provider(provider);
        }
    }

    Ok(())
}

/// Shutdown the tracer, flushing any pending spans.
pub fn shutdown_tracer() {
    global::shutdown_tracer_provider();
}

// ============================================================================
// W3C Trace Context Propagation
// ============================================================================

/// Extract trace context from W3C `traceparent` header.
///
/// Format: `00-<trace-id>-<span-id>-<trace-flags>`
pub fn extract_traceparent(traceparent: &str) -> Option<(String, String, u8)> {
    let parts: Vec<&str> = traceparent.split('-').collect();
    if parts.len() != 4 || parts[0] != "00" {
        return None;
    }
    let trace_id = parts[1].to_string();
    let span_id = parts[2].to_string();
    // Validate trace_id and span_id are valid hex strings
    if !trace_id.chars().all(|c| c.is_ascii_hexdigit())
        || !span_id.chars().all(|c| c.is_ascii_hexdigit())
    {
        return None;
    }
    let flags = u8::from_str_radix(parts[3], 16).ok()?;
    Some((trace_id, span_id, flags))
}

/// Format a `traceparent` header value.
pub fn format_traceparent(trace_id: &str, span_id: &str, flags: u8) -> String {
    format!("00-{}-{}-{:02x}", trace_id, span_id, flags)
}

/// Generate a new trace ID (32 hex chars).
pub fn generate_trace_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("{:032x}", now)
}

/// Generate a new span ID (16 hex chars).
pub fn generate_span_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("{:016x}", now)
}

// ============================================================================
// Request Span Helper
// ============================================================================

/// Create a request span with standard attributes.
pub fn create_request_span(
    request_id: &str,
    model: &str,
    provider: &str,
) -> opentelemetry::global::BoxedSpan {
    use opentelemetry::trace::SpanKind;

    let tracer = global::tracer("quota-router");
    let span = tracer
        .span_builder("request")
        .with_kind(SpanKind::Server)
        .with_attributes(vec![
            KeyValue::new("request.id", request_id.to_string()),
            KeyValue::new("model", model.to_string()),
            KeyValue::new("provider", provider.to_string()),
        ])
        .start(&tracer);
    span
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tracing_config_default() {
        let config = TracingConfig::default();
        assert_eq!(config.otlp_endpoint, "http://localhost:4317");
        assert_eq!(config.sampling_rate, 1.0);
        assert_eq!(config.exporter, "otlp");
        assert_eq!(config.propagation, "w3c");
        assert_eq!(config.service_name, "quota-router");
        assert!(config.deployment_environment.is_none());
    }

    #[test]
    fn test_extract_traceparent_valid() {
        let header = "00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01";
        let (trace_id, span_id, flags) = extract_traceparent(header).unwrap();
        assert_eq!(trace_id, "0af7651916cd43dd8448eb211c80319c");
        assert_eq!(span_id, "b7ad6b7169203331");
        assert_eq!(flags, 1);
    }

    #[test]
    fn test_extract_traceparent_invalid_version() {
        let header = "01-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01";
        assert!(extract_traceparent(header).is_none());
    }

    #[test]
    fn test_extract_traceparent_invalid_format() {
        assert!(extract_traceparent("invalid").is_none());
        assert!(extract_traceparent("00-trace-span-01").is_none());
    }

    #[test]
    fn test_format_traceparent() {
        let result = format_traceparent("abc123", "def456", 1);
        assert_eq!(result, "00-abc123-def456-01");
    }

    #[test]
    fn test_generate_trace_id() {
        let id = generate_trace_id();
        assert_eq!(id.len(), 32);
        assert!(id.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_generate_span_id() {
        let id = generate_span_id();
        assert_eq!(id.len(), 16);
        assert!(id.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_tracing_config_deserialize() {
        let json = r#"{
            "otlp_endpoint": "http://jaeger:4317",
            "sampling_rate": 0.5,
            "exporter": "otlp",
            "propagation": "w3c",
            "service_name": "my-router",
            "deployment_environment": "staging"
        }"#;
        let config: TracingConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.otlp_endpoint, "http://jaeger:4317");
        assert_eq!(config.sampling_rate, 0.5);
        assert_eq!(config.deployment_environment, Some("staging".to_string()));
    }
}
