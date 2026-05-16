# RFC-0937: Prometheus Metrics Endpoint

## Status: Accepted

## Summary

Expose Prometheus metrics for quota-router, matching any-llm's gateway metrics. This enables monitoring of request rates, latencies, errors, and costs.

## Motivation

any-llm's gateway exposes Prometheus metrics for monitoring. quota-router has no metrics endpoint. This RFC adds a `/metrics` endpoint with standard Prometheus format.

## Specification

### 1. Metrics Categories

#### Request Metrics
```
quota_router_requests_total{provider="openai",model="gpt-4o",status="success"} 1234
quota_router_request_duration_seconds{provider="openai",model="gpt-4o"} 0.5
quota_router_request_tokens{provider="openai",model="gpt-4o",type="input"} 5000
quota_router_request_tokens{provider="openai",model="gpt-4o",type="output"} 1000
```

#### Rate Limiting Metrics
```
quota_router_rate_limit_hits_total{key_prefix="sk-qr-ab",type="rpm"} 5
quota_router_rate_limit_remaining{key_prefix="sk-qr-ab",type="rpm"} 95
```

**Security note:** Use `key_prefix` (first 8 chars) instead of full key to avoid leaking secrets in metrics. Same for entity_id — use UUID prefix.

#### Budget Metrics
```
quota_router_budget_spend_microdollars{entity_type="user",entity_prefix="a1b2c3d4"} 85500000
quota_router_budget_limit_microdollars{entity_type="user",entity_prefix="a1b2c3d4"} 100000000
quota_router_budget_alerts_total{entity_type="user"} 2
```

**Note:** Budget values are in microdollars (1 USD = 1,000,000). Use integer values, not float USD.

#### Provider Metrics
```
quota_router_provider_errors_total{provider="openai",error_type="rate_limit"} 10
quota_router_provider_latency_seconds{provider="openai",quantile="0.5"} 0.3
quota_router_provider_latency_seconds{provider="openai",quantile="0.95"} 1.2
```

#### Routing Metrics
```
quota_router_routing_decisions_total{strategy="simple_shuffle"} 500
quota_router_cooldown_activations_total{provider="openai"} 3
quota_router_fallback_activations_total{type="general"} 2
```

#### Cache Metrics
```
quota_router_cache_hits_total{cache="response"} 100
quota_router_cache_misses_total{cache="response"} 50
```

#### Pre-call Check Metrics
```
quota_router_precall_check_failures_total{check="context_window"} 5
quota_router_precall_check_failures_total{check="tag_filter"} 2
quota_router_precall_check_failures_total{check="health"} 1
```

### 2. Implementation

```rust
use prometheus::{Encoder, Gauge, Histogram, IntCounter, Registry};

pub struct Metrics {
    registry: Registry,

    // Request metrics (with labels: provider, model, status)
    requests_total: IntCounterVec,
    request_duration: HistogramVec,
    request_tokens_input: IntCounterVec,
    request_tokens_output: IntCounterVec,

    // Rate limit metrics (with labels: key_prefix, type)
    rate_limit_hits: IntCounterVec,

    // Budget metrics (with labels: entity_type, entity_prefix)
    budget_spend: GaugeVec,
    budget_limit: GaugeVec,
    budget_alerts: IntCounterVec,

    // Provider metrics (with labels: provider, error_type)
    provider_errors: IntCounterVec,
    provider_latency: HistogramVec,

    // Routing metrics (with labels: strategy, provider)
    routing_decisions: IntCounterVec,
    cooldown_activations: IntCounterVec,
    fallback_activations: IntCounterVec,

    // Cache metrics (with labels: cache)
    cache_hits: IntCounterVec,
    cache_misses: IntCounterVec,

    // Pre-call check metrics (with labels: check)
    precall_check_failures: IntCounterVec,
}

impl Metrics {
    pub fn new() -> Self {
        let registry = Registry::new();

        let requests_total = IntCounterVec::new(
            Opts::new("quota_router_requests_total", "Total number of requests"),
            &["provider", "model", "status"]
        ).unwrap();

        let request_duration = HistogramVec::new(
            HistogramOpts::new("quota_router_request_duration_seconds", "Request duration in seconds")
                .buckets(vec![0.01, 0.05, 0.1, 0.5, 1.0, 5.0, 10.0]),
            &["provider", "model"]
        ).unwrap();

        // ... register all metrics with registry.register(Box::new(metric.clone())).unwrap();

        Self { registry, requests_total, request_duration, /* ... */ }
    }

    pub fn render(&self) -> Result<String, MetricsError> {
        let encoder = TextEncoder::new();
        let metric_families = self.registry.gather();
        let mut buffer = Vec::new();
        encoder.encode(&metric_families, &mut buffer)
            .map_err(|e| MetricsError::EncodeError(e.to_string()))?;
        String::from_utf8(buffer)
            .map_err(|e| MetricsError::Utf8Error(e.to_string()))
    }
}
```

### 3. Metrics Endpoint

```
GET /metrics
```

Returns Prometheus text format.

**Port and auth:** The `/metrics` endpoint should be served on the same port as the proxy but BYPASSES auth middleware (Prometheus scrapes it without API keys). The auth bypass is implemented by checking the request path before the auth middleware — if path is `/metrics`, skip auth and return metrics directly. This requires adding a `bypass_paths: Vec<String>` config option to GatewayConfig.

**Security recommendation:** Since `/metrics` exposes operational metadata (key prefixes, provider names, budget entity prefixes) without authentication, restrict access at the network level:
- Bind metrics to localhost only (`127.0.0.1`) if Prometheus runs on the same host
- Use firewall rules to allow only the Prometheus scraper IP
- Alternatively, serve `/metrics` on a separate management port (e.g., `:9090`) that is not publicly exposed
- The `key_prefix` and `entity_prefix` labels mitigate secret leakage, but operational metadata is still visible

### 4. Middleware Integration

Use hyper-compatible integration (codebase uses raw hyper with `service_fn`, NOT axum):
```rust
// In proxy.rs — integrate with existing handle_request()
async fn handle_request_with_metrics(
    req: Request<hyper::body::Incoming>,
    metrics: Arc<Metrics>,
    // ... other params
) -> Result<Response<impl Body>, Infallible> {
    let start = Instant::now();

    // Note: Use .with_label_values(&[provider, model, status]) after response
    // metrics.requests_total.with_label_values(&[...]).inc();

    // Extract key prefix for metrics (safe — no full key exposure)
    let key_prefix = extract_key_from_request(&req)
        .ok()
        .flatten()
        .map(|k| k.chars().take(7).collect::<String>())  // 7 chars to match existing middleware.rs convention
        .unwrap_or_default();

    // ... existing request handling ...

    let duration = start.elapsed();
    metrics.request_duration.observe(duration.as_secs_f64());

    response
}
```

### 5. Configuration

```yaml
metrics:
  enabled: true
  endpoint: /metrics
  # Optional: push to Prometheus Pushgateway
  push_gateway:
    enabled: false
    url: http://pushgateway:9091
    job: quota-router
    interval_seconds: 15
```

## Dependencies

- GatewayConfig extension: `bypass_paths: Vec<String>` must be added to GatewayConfig (the top-level server config, NOT RouterConfig). This field is added as part of this RFC's implementation.
- Optional/soft dependencies: RFC-0933 (rate limit metrics), RFC-0934 (budget metrics), RFC-0936 (pre-call check metrics) — metrics for these features are only available when the corresponding RFC is implemented

## Test Plan

1. /metrics endpoint returns 200 with Prometheus format
2. Request counter increments on each request
3. Duration histogram records accurate latencies
4. Token counters track input/output tokens
5. Rate limit hits are counted
6. Budget spend is tracked
7. Provider errors are categorized
8. Metrics survive restart (stoolap persistence optional)
9. `/metrics` returns 200 without API key when `bypass_paths` includes `/metrics`
10. `/v1/chat/completions` still requires auth when `bypass_paths` is configured
