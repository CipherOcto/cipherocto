# RFC-0905 (Economics): Observability and Logging

## Status

Planned

## Authors

- Author: @cipherocto

## Summary

Define the observability system for the enhanced quota router, including structured logging, metrics export, tracing, and alerting.

## Dependencies

**Requires:**

**Optional:**

- RFC-0900 (Economics): AI Quota Marketplace Protocol
- RFC-0901 (Economics): Quota Router Agent Specification
- RFC-0902: Multi-Provider Routing (for latency metrics)
- RFC-0903: Virtual API Key System (for auth metrics)
- RFC-0904: Real-Time Cost Tracking (for spend metrics)


## Why Needed

The enhanced quota router needs observability for:

- Debugging production issues
- Monitoring system health
- Alerting on anomalies
- Performance optimization
- Audit compliance

## Scope

### In Scope

- Structured JSON logging
- Prometheus metrics export
- OpenTelemetry tracing
- Log levels (debug, info, warn, error)
- Request/response logging
- Error tracking

### Out of Scope

- Third-party log aggregation (Datadog, Splunk)
- Custom dashboards (future)
- Anomaly detection (future)

## Design Goals

| Goal | Target | Metric |
|------|--------|--------|
| G1 | <1ms log overhead | Logging latency |
| G2 | Prometheus /metrics | Metrics endpoint |
| G3 | JSON structured logs | Log format |
| G4 | Trace context propagation | Distributed tracing |

## Specification

### Log Levels

```rust
enum LogLevel {
    Debug,  // Detailed debugging info
    Info,   // General info
    Warn,   // Warning conditions
    Error,  // Error conditions
}
```

### Structured Log Format

```json
{
  "timestamp": "2026-03-12T10:30:00.000Z",
  "level": "info",
  "component": "router",
  "event": "request_routed",
  "trace_id": "abc123",
  "key_id": "key-uuid",
  "provider": "openai",
  "model": "gpt-4o",
  "latency_ms": 150,
  "status": "success"
}
```

### Key Metrics

```rust
// Request metrics
- requests_total (counter)
- requests_in_flight (gauge)
- request_duration_seconds (histogram)

// Provider metrics
- provider_requests_total (counter)
- provider_latency_seconds (histogram)
- provider_errors_total (counter)

// Cost metrics
- spend_total (counter)
- budget_remaining (gauge)

// System metrics
- active_connections (gauge)
- memory_usage_bytes (gauge)
```

### Metrics Endpoint

```yaml
# Config
general_settings:
  metrics_enabled: true
  metrics_port: 9090

# Prometheus format at /metrics
GET /metrics
```

### Tracing

Support OpenTelemetry for distributed tracing:

```rust
// Trace context propagation
fn handle_request(req: Request) -> Response {
    let span = tracer::span("handle_request")
        .with_parent(req.headers().get("traceparent"));

    span.record("key_id", &key_id);
    span.record("provider", &provider);

    // ... handle request
}
```

### Alerting

```yaml
# Alert configuration
alerting:
  slack:
    enabled: true
    webhook_url: "${SLACK_WEBHOOK_URL}"

  alerts:
    - name: high_error_rate
      condition: error_rate > 0.05
      threshold: 5m
      severity: critical

    - name: budget_exhausted
      condition: budget_remaining < 0
      severity: warning
```

### API Endpoints

```rust
// Health and metrics
GET  /health          // Basic health check
GET  /health/ready   // Readiness probe
GET  /health/live   // Liveness probe
GET  /metrics       // Prometheus metrics
GET  /debug/pprof   // pprof profiles
```

### LiteLLM Compatibility

> **Critical:** Must track LiteLLM's logging callbacks.

Reference LiteLLM's observability:
- `litellm.success_callback` for logging
- `litellm.failure_callback` for error logging
- Custom logger support
- Langfuse, DataDog integrations

## Key Files to Modify

| File | Change |
|------|--------|
| `crates/quota-router-cli/src/logging.rs` | New - structured logging |
| `crates/quota-router-cli/src/metrics.rs` | New - Prometheus metrics |
| `crates/quota-router-cli/src/tracing.rs` | New - OpenTelemetry tracing |
| `crates/quota-router-cli/src/alerting.rs` | New - alerting |

## Future Work

- F1: Log aggregation integration
- F2: Custom dashboards
- F3: Anomaly detection
- F4: Audit logging

## Rationale

Observability is essential for:

1. **Production debugging** - Understand issues quickly
2. **Monitoring** - Track system health
3. **Alerting** - Respond to incidents
4. **Compliance** - Audit trails
5. **LiteLLM migration** - Match logging callbacks

---

**Planned Date:** 2026-03-12
**Related Use Case:** Enhanced Quota Router Gateway
**Related Research:** LiteLLM Analysis and Quota Router Comparison
