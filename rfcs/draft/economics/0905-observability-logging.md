# RFC-0905 (Economics): Observability and Logging

## Status

Draft

## Authors

- Author: @cipherocto

## Maintainers

- Maintainer: @cipherocto

## Summary

Define the observability system for the enhanced quota router, including structured logging, OpenTelemetry tracing, and health endpoints. Prometheus metrics are specified in RFC-0937 (Accepted). Callback integrations are specified in RFC-0947.

## Dependencies

**Requires:**

- RFC-0937 (Economics): Prometheus Metrics Endpoint (Accepted) — provides `/metrics` endpoint

**Optional:**

- RFC-0947 (Economics): Callback System — provides alerting and third-party integrations
- RFC-0902: Multi-Provider Routing (for latency metrics)
- RFC-0903: Virtual API Key System (for auth metrics)
- RFC-0904: Real-Time Cost Tracking (for spend metrics)

## Motivation

The enhanced quota router needs observability for:

- Debugging production issues via structured logs
- Monitoring system health via health endpoints
- Distributed tracing via OpenTelemetry
- Performance optimization via trace analysis
- Audit compliance via request logging

## Design Goals

| Goal | Target | Metric |
|------|--------|--------|
| G1 | <1ms log overhead | Logging latency (async buffered) |
| G2 | JSON structured logs | Log format (NDJSON) |
| G3 | Trace context propagation | W3C Traceparent header |
| G4 | Health probes | K8s-compatible /healthz endpoints |

## Scope

### In Scope

- Structured JSON logging (NDJSON to stdout)
- OpenTelemetry tracing (OTLP export)
- Health endpoints (liveness, readiness)
- Request/response logging with redaction
- Log sampling and rate limiting

### Out of Scope

- Prometheus metrics (see RFC-0937)
- Alerting and callbacks (see RFC-0947)
- Third-party log aggregation (Datadog, Splunk) — via RFC-0947 callbacks
- Custom dashboards (future)
- Anomaly detection (future)

## Specification

### 1. Structured Logging

#### Log Levels

```rust
enum LogLevel {
    Debug,  // Detailed debugging info (dev only)
    Info,   // General operational info
    Warn,   // Warning conditions
    Error,  // Error conditions
}
```

#### NDJSON Format

Each log line is a single JSON object written to stdout:

```json
{"timestamp":"2026-05-17T10:30:00.000Z","level":"info","component":"proxy","event":"request_completed","trace_id":"abc123","span_id":"def456","request_id":"req-789","provider":"openai","model":"gpt-4o","status":"success","latency_ms":150,"input_tokens":500,"output_tokens":200}
```

**Required fields:** `timestamp`, `level`, `component`, `event`
**Optional fields:** `trace_id`, `span_id`, `request_id`, `provider`, `model`, `status`, `latency_ms`, `input_tokens`, `output_tokens`, `error`

#### Log Events

| Event | Level | When |
|-------|-------|------|
| `request_started` | debug | Request received |
| `request_completed` | info | Request finished successfully |
| `request_failed` | error | Request finished with error |
| `key_validated` | debug | API key validated |
| `provider_selected` | debug | Provider selected by routing |
| `rate_limit_hit` | warn | Rate limit triggered |
| `budget_warning` | warn | Budget threshold exceeded |
| `cache_hit` | debug | Cache hit |
| `cache_miss` | debug | Cache miss |

#### PII Redaction

Log events MUST NOT contain:
- Full API keys (use prefix only: `sk-qr-ab...`)
- User message content (use token count only)
- Response content (use token count only)
- IP addresses (use GeoIP region if needed)

#### Async Buffering

```rust
use tokio::sync::mpsc;
use std::io::{self, Write};

struct LogWriter {
    sender: mpsc::Sender<LogEvent>,
}

impl LogWriter {
    fn new(buffer_size: usize) -> Self {
        let (sender, mut receiver) = mpsc::channel(buffer_size);
        tokio::spawn(async move {
            let mut stdout = io::stdout();
            while let Some(event) = receiver.recv().await {
                // Write to stdout (non-blocking)
                // Note: NDJSON serializer omits trailing newline; we add it explicitly
                let mut json = serde_json::to_string(&event).unwrap();
                json.push('\n');
                let _ = stdout.write_all(json.as_bytes());
            }
        });
        Self { sender }
    }
}
```

**Default buffer size:** 10,000 events. When buffer is full, events are dropped with a `log_dropped_total` counter.

**Note on log rotation:** Log rotation is handled by the container runtime (Docker, K8s). The router writes to stdout only. For non-containerized deployments, configure log rotation via systemd-journald or an external log agent.

#### Log Sampling

Under high load, logging all requests is expensive. Support sampling:

```yaml
logging:
  level: info
  sample_rate: 1.0  # 1.0 = log all, 0.1 = log 10%
  # Always log errors regardless of sample rate
  always_log_errors: true
```

**Sampling strategy:** Reservoir sampling per second. If `sample_rate=0.1`, log 10% of requests per second, always including errors.

### 2. OpenTelemetry Tracing

#### OTLP Export

```rust
use opentelemetry::{trace::Tracer, KeyValue};
use opentelemetry_otlp::WithExportConfig;

fn init_tracer(config: &TracingConfig) -> Tracer {
    opentelemetry_otlp::new_pipeline()
        .tracing()
        .with_exporter(
            opentelemetry_otlp::new_exporter()
                .tonic()
                .with_endpoint(&config.otlp_endpoint)
        )
        .with_trace_config(
            opentelemetry::sdk::trace::config()
                .with_resource(opentelemetry::sdk::Resource::new(vec![
                    KeyValue::new("service.name", "quota-router"),
                    KeyValue::new("service.version", env!("CARGO_PKG_VERSION")),
                ]))
        )
        .install_batch()  // Use install_batch() for production (non-blocking export)
        .expect("Failed to install tracer")
}
```

**Note:** Use `install_simple()` for development/testing only. Production deployments MUST use `install_batch()` which uses a background thread for non-blocking span export. `install_simple()` blocks on every export and will add latency to request handling.

#### Context Propagation

**W3C Trace Context** (standard):

```
traceparent: 00-<trace-id>-<span-id>-<trace-flags>
```

Extract from incoming requests, propagate to provider requests:

```rust
fn handle_request(req: Request) -> Response {
    let parent_ctx = opentelemetry::global::get_text_map_propagator(|propagator| {
        propagator.extract(&HeaderExtractor(req.headers()))
    });

    let span = tracer.span_builder("handle_request")
        .with_parent_context(parent_ctx)
        .with_attributes(vec![
            KeyValue::new("http.method", req.method().to_string()),
            KeyValue::new("http.url", req.uri().to_string()),
        ])
        .start(&tracer);

    // ... handle request

    span.end();
}
```

#### Sampling Strategy

```yaml
tracing:
  enabled: false
  otlp_endpoint: "http://localhost:4317"
  sample_rate: 1.0  # 1.0 = trace all, 0.01 = trace 1%
  # Probabilistic sampling (parent-based)
  sampler: parent_based  # parent_based | always_on | always_off | trace_id_ratio
```

**Default:** Tracing disabled (opt-in). When enabled, `parent_based` sampler respects upstream decision.

#### Resource Attributes

| Attribute | Value |
|-----------|-------|
| `service.name` | `quota-router` |
| `service.version` | Cargo package version |
| `deployment.environment` | `production` / `staging` / `development` |

### 3. Health Endpoints

K8s-compatible health probes:

```
GET /healthz          — Liveness probe (is the process alive?)
GET /healthz/ready    — Readiness probe (is it ready to serve traffic?)
```

#### Liveness Response

```json
{"status": "ok"}
```

Returns 200 if the process is alive. Always returns 200 (liveness = process is running).

#### Readiness Response

```json
{
  "status": "ok",
  "checks": {
    "stoolap": "ok",
    "config": "ok",
    "providers": "ok"
  }
}
```

Returns 200 if ready to serve. Returns 503 if any check fails:

```json
{
  "status": "degraded",
  "checks": {
    "stoolap": "ok",
    "config": "ok",
    "providers": "error: no healthy providers"
  }
}
```

**Readiness checks:**
- `stoolap`: Can connect to stoolap database
- `config`: Config is valid and loaded
- `providers`: At least one provider is healthy

### 4. Request/Response Logging

Log request/response metadata (NOT content):

```json
{
  "event": "request_completed",
  "request_id": "req-abc123",
  "method": "POST",
  "path": "/v1/chat/completions",
  "provider": "openai",
  "model": "gpt-4o",
  "status": "success",
  "latency_ms": 150,
  "input_tokens": 500,
  "output_tokens": 200,
  "key_prefix": "sk-qr-ab",
  "status_code": 200
}
```

**Never log:**
- Request body (contains user messages)
- Response body (contains LLM output)
- Full API keys
- Authorization headers

### 5. Error Tracking

Log errors with structured context:

```json
{
  "event": "request_failed",
  "level": "error",
  "error_type": "provider_error",
  "error_message": "Rate limit exceeded",
  "provider": "openai",
  "model": "gpt-4o",
  "retry_count": 1,
  "trace_id": "abc123"
}
```

### 6. Configuration

```yaml
logging:
  level: info              # debug | info | warn | error
  format: json             # json | text
  sample_rate: 1.0         # 0.0-1.0
  always_log_errors: true  # log errors regardless of sample_rate
  buffer_size: 10000       # async buffer capacity

tracing:
  enabled: false
  otlp_endpoint: "http://localhost:4317"
  sample_rate: 1.0
  sampler: parent_based    # parent_based | always_on | always_off | trace_id_ratio
```

## Key Files to Modify

| File | Change |
|------|--------|
| `crates/quota-router-core/src/logging.rs` | New — structured NDJSON logging |
| `crates/quota-router-core/src/tracing.rs` | New — OpenTelemetry integration |
| `crates/quota-router-core/src/health.rs` | New — health endpoints |
| `crates/quota-router-core/src/proxy.rs` | Integrate logging and tracing |
| `crates/quota-router-core/src/config.rs` | Add logging/tracing config |

## Adversarial Review

### Threat Analysis

| Threat | Impact | Mitigation |
|--------|--------|------------|
| PII in logs | High (GDPR/SOC2 violation) | Redaction rules, never log message content |
| Log flooding (DoS) | Medium (resource exhaustion) | Async buffer with bounded capacity, sampling |
| Trace context spoofing | Low (misleading traces) | Validate trace ID format, ignore invalid |
| Health endpoint info leak | Low (operational metadata) | Minimal info in health responses |
| Log injection | Medium (log forgery) | JSON serialization escapes special chars |

### Design Decisions

| Decision | Rationale |
|----------|-----------|
| NDJSON to stdout | Standard for container logging (Docker, K8s) |
| W3C Trace Context | Industry standard, supported by all OTel exporters |
| Separate /healthz path | K8s convention, avoids conflict with /metrics |
| Async buffered logging | Non-blocking, prevents log writes from affecting request latency |
| Sampling support | Essential for high-throughput deployments |

## Alternatives Considered

| Approach | Pros | Cons |
|----------|------|------|
| Structured logging via tracing crate | Rust ecosystem standard | More complex, heavier |
| Log to file instead of stdout | Persistent logs | Container logging expects stdout |
| B3 propagation | Zipkin ecosystem | W3C is more widely adopted |
| Sync logging | Simpler | Blocks request handling |

## Performance Targets

| Metric | Target | Notes |
|--------|--------|-------|
| Log write latency | <1ms | Async buffered, non-blocking |
| Trace span creation | <0.1ms | Minimal overhead |
| Health check latency | <5ms | Simple status checks |
| Log buffer memory | <10MB | 10K events × ~1KB each |

## Security Considerations

- **PII redaction:** Never log message content, full keys, or IPs
- **Trace propagation:** Validate trace ID format, reject malformed
- **Health endpoints:** No authentication required (operational), but minimal info exposure
- **Log injection:** JSON serialization prevents injection
- **Resource limits:** Bounded buffer prevents memory exhaustion

## Compatibility

- **Backward:** Logging is additive, no breaking changes
- **Forward:** New log events can be added without breaking consumers
- **K8s:** Health endpoints follow K8s probe conventions
- **OTel:** Standard OTLP export, compatible with Jaeger, Zipkin, Grafana Tempo

## Test Vectors

### Log Event

```json
{
  "timestamp": "2026-05-17T10:30:00.000Z",
  "level": "info",
  "component": "proxy",
  "event": "request_completed",
  "trace_id": "4bf92f3577b34da6a3ce929d0e0e4736",
  "span_id": "00f067aa0ba902b7",
  "request_id": "req-abc123",
  "provider": "openai",
  "model": "gpt-4o",
  "status": "success",
  "latency_ms": 150,
  "input_tokens": 500,
  "output_tokens": 200,
  "key_prefix": "sk-qr-ab",
  "status_code": 200
}
```

### Traceparent Header

```
traceparent: 00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01
```

### Health Response

```json
{
  "status": "ok",
  "checks": {
    "stoolap": "ok",
    "config": "ok",
    "providers": "ok"
  }
}
```

## Future Work

- F1: Log aggregation integration (via RFC-0947 callbacks)
- F2: Custom dashboards
- F3: Anomaly detection
- F4: Audit logging with retention policy
- F5: Distributed trace correlation across multiple router instances

## Rationale

Observability is essential for:

1. **Production debugging** — Structured logs with trace IDs enable fast root cause analysis
2. **Monitoring** — Health endpoints enable K8s liveness/readiness probes
3. **Tracing** — OpenTelemetry enables distributed tracing across provider calls
4. **Compliance** — Audit trails for request logging (with PII redaction)
5. **Performance** — Trace analysis identifies latency bottlenecks

## Version History

| Version | Date | Changes |
|---------|------|---------|
| v1 | 2026-03-12 | Initial (Planned) |
| v2 | 2026-05-17 | Moved to Draft — deferred metrics to RFC-0937, added tracing spec, health endpoints, logging events, adversarial review |

## Related RFCs

- RFC-0937: Prometheus Metrics Endpoint (Accepted) — `/metrics` endpoint
- RFC-0947: Callback System (Draft) — alerting and third-party integrations
- RFC-0944: Structured Logging (completed) — existing logging implementation

## Related Use Cases

- [Enhanced Quota Router Gateway](../../docs/use-cases/enhanced-quota-router-gateway.md)
