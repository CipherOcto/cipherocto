# Mission: 0937-a — Prometheus Metrics

## Status

Open

## RFC

RFC-0937 (Economics): Prometheus Metrics Endpoint

## Dependencies

- Mission-0928-a: Deployment Config Schema (GatewayConfig access)

**Note:** RFC-0937 specifies `bypass_paths: Vec<String>` must be added to GatewayConfig. This field does not exist yet and is added as new code in this mission.

## Context

RFC-0937 specifies a `/metrics` endpoint exposing Prometheus metrics. This mission implements the metrics collection and endpoint.

**Note:** Codebase uses raw hyper with `service_fn`, NOT axum. Do NOT use axum patterns (Next, request.extensions()). The `Metrics` struct should be passed as a parameter to `handle_request()`, not injected via extensions.

**Note:** `extract_key_from_request(&self, request)` is a method on `GatewayAuth` (middleware.rs:32), not a standalone function. The metrics middleware must have access to a `GatewayAuth` instance to extract key prefixes.

## Acceptance Criteria

### Config

- [ ] Add `bypass_paths: Vec<String>` to GatewayConfig (for auth bypass)
- [ ] Add `/metrics` to default bypass_paths

### Metrics Struct

- [ ] `Metrics` struct with Prometheus counters, histograms, gauges
- [ ] Request metrics: requests_total, request_duration, request_tokens
- [ ] Rate limit metrics: rate_limit_hits_total
- [ ] Budget metrics: budget_spend_microdollars, budget_alerts_total
- [ ] Provider metrics: provider_errors_total, provider_latency
- [ ] Routing metrics: routing_decisions_total, cooldown_activations, fallback_activations
- [ ] Cache metrics: cache_hits_total, cache_misses_total
- [ ] Pre-call check metrics: precall_check_failures_total

### Endpoint

- [ ] `GET /metrics` returns Prometheus text format
- [ ] Use `prometheus` crate for metrics collection
- [ ] Use `TextEncoder` for output

### Security

- [ ] Use `key_prefix` (first 7 chars, per middleware.rs and RFC-0937) instead of full key in metrics
- [ ] Use `entity_prefix` instead of full entity_id

### Tests

- [ ] /metrics endpoint returns 200 with Prometheus format
- [ ] Request counter increments on each request
- [ ] Duration histogram records accurate latencies

## Key Files

- `crates/quota-router-core/src/metrics.rs` — new file (Metrics struct)
- `crates/quota-router-core/src/proxy.rs` — wire metrics middleware

## Notes

This is a new module. The `prometheus` crate should be added to Cargo.toml. The metrics should be integrated into the proxy request path via middleware.

**Dependencies:** Add `prometheus` crate to Cargo.toml. Import: `use prometheus::{Counter, Gauge, Histogram, IntCounter, Opts, Registry, TextEncoder};`

**bypass_paths:** `GatewayConfig` needs a new `bypass_paths: Vec<String>` field for paths that skip metrics collection (e.g., `/health`, `/ready`). Default: `["/health", "/ready"]`.

**Metrics registration:** Create a `Metrics` struct holding all metric instances. Register on `GatewayConfig` load. Expose via `/metrics` endpoint using `TextEncoder::new().encode_to_string()`.

**handle_request integration:** The metrics middleware wraps the existing `handle_request()` call. It extracts the key prefix BEFORE the request is processed, records timing via `Instant::now()`, and increments counters on response.
