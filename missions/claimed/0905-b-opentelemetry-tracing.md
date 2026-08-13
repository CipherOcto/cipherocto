# Mission: 0905-b — OpenTelemetry Tracing

## Status

LANDED 2026-08-13. Originally filed pre-public-launch; now fully landed.

**Landing scope:** `crates/quota-router-core/src/tracing.rs` (296 lines, 9 unit tests) — full OpenTelemetry module with `TracingConfig` (endpoint + sampling_rate + exporter + propagation + service_name + deployment_environment), `init_tracer()` with OTLP gRPC exporter (`opentelemetry-otlp` with `tonic` feature + `opentelemetry_sdk` with `rt-tokio`), `Sampler::ParentBased(TraceIdRatioBased)` for parentbased_traceidratio sampling, resource attributes (service.name, service.version, deployment.environment), W3C Trace Context (`extract_traceparent()` + `format_traceparent()`), `generate_trace_id()` (32 hex) + `generate_span_id()` (16 hex). `TracingConfig` re-exported at `config.rs:11` + embedded in main `Config` struct at `config.rs:563` + 5 default-population sites.

**Proxy wiring (RFC-0905 §W3C Trace Context):** `proxy.rs handle_request` extracts the `traceparent` header at the request entry; if absent or malformed, generates a fresh `trace_id` + `span_id` and synthesizes a valid traceparent. The outgoing provider request (`/v1/chat/completions` primary path) injects the traceparent header so the upstream provider can correlate this hop into the distributed trace. 9 unit tests cover config defaults + JSON deserialization + traceparent parse/format + invalid-format rejection + traceparent round-trip (format → extract → recover same ids).

9 tracing tests + 228 proxy tests pass; clippy `-D warnings` clean.

## RFC

RFC-0905 (Economics): Observability and Logging

## Dependencies

- Mission-0905-a: Structured Logging

## Acceptance Criteria

- [x] Integrate `opentelemetry-otlp` crate for OTLP export — **LANDED** (`Cargo.toml:42` `opentelemetry-otlp = { version = "0.27", features = ["tonic", "trace"] }`; `tracing.rs:119-123` builds `SpanExporter::builder().with_tonic().with_endpoint(...)`)
- [x] Implement W3C Trace Context propagation (traceparent header) — **LANDED** (`extract_traceparent()` at `tracing.rs` + `format_traceparent()` + invalid-format/version tests)
- [x] Implement sampling strategy (parentbased_traceidratio) — **LANDED** (`tracing.rs:106-108` `Sampler::ParentBased(Box::new(sampler))` wrapping `Sampler::TraceIdRatioBased`)
- [x] Configure resource attributes — **LANDED** (`tracing.rs:87-94` builds `KeyValue`s for `service.name` + `service.version` + `deployment.environment`)
- [x] Add `TracingConfig` to `config.rs` — **LANDED** (`config.rs:11` re-export + `config.rs:563` `pub tracing: TracingConfig` field + 5 default-population sites)
- [x] Generate trace_id and span_id for each request — **LANDED** (`generate_trace_id()` 32-hex + `generate_span_id()` 16-hex + tests verify format)
- [x] Inject traceparent header into outgoing provider requests — **LANDED** (`proxy.rs handle_request` injects via `.header("traceparent", &outgoing_traceparent)` at the primary `/v1/chat/completions` outgoing req_builder site)
- [x] Extract traceparent header from incoming requests — **LANDED** (`proxy.rs handle_request` extracts via `req.headers().get("traceparent")` + `extract_traceparent()` validation; falls back to fresh-id synthesis if absent or malformed)
- [x] Clippy passes with zero warnings — **VERIFIED** (`cargo clippy -p quota-router-core --all-targets --features full -- -D warnings` clean)
- [x] All existing tests pass — **VERIFIED** (9 tracing tests pass including new `test_traceparent_roundtrip`; 228 proxy tests pass; 30 callback tests pass)

## Claimant

(unclaimed)

## Pull Request

#

## Notes

Key files:

- `crates/quota-router-core/src/tracing.rs` — OpenTelemetry module (296 lines, 9 unit tests)
- `crates/quota-router-core/src/config.rs` — TracingConfig integration (`config.rs:11` re-export + `config.rs:563` field)
- `crates/quota-router-core/src/proxy.rs` — W3C Trace Context propagation at request entry + outgoing provider injection

## Version History

| Version | Date       | Change                                                                                                                                                                                                                                                                                                                                                   |
| ------- | ---------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| v0.1    | (filed)    | Mission filed. 10 ACs covering OpenTelemetry OTLP export + W3C Trace Context + parentbased_traceidratio sampling + resource attributes + TracingConfig + trace/span id generation + incoming extract + outgoing inject + clippy + tests.                                                                                                                 |
| v0.2    | 2026-08-13 | **LANDED.** All 10/10 ACs verified. `tracing.rs` (296 lines) with `TracingConfig` + `init_tracer()` + `Sampler::ParentBased(TraceIdRatioBased)` + W3C Trace Context helpers. `proxy.rs handle_request` extracts incoming `traceparent` (or synthesizes fresh id) + injects outgoing. 9 tracing tests + 228 proxy tests pass; clippy `-D warnings` clean. |

Last Updated: 2026-08-13
Version: 0.2 (LANDED)
