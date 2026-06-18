# Mission: 0905-b — OpenTelemetry Tracing

## Status

Open

## RFC

RFC-0905 (Economics): Observability and Logging

## Dependencies

- Mission-0905-a: Structured Logging

## Acceptance Criteria

- [ ] Integrate `opentelemetry-otlp` crate for OTLP export
- [ ] Implement W3C Trace Context propagation (traceparent header)
- [ ] Implement sampling strategy (parentbased_traceidratio)
- [ ] Configure resource attributes (service.name, service.version, deployment.environment)
- [ ] Add `TracingConfig` to `config.rs` (endpoint, sampling_rate, exporter, propagation)
- [ ] Generate trace_id and span_id for each request
- [ ] Inject traceparent header into outgoing provider requests
- [ ] Extract traceparent header from incoming requests
- [ ] Clippy passes with zero warnings
- [ ] All existing tests pass

## Claimant

(unclaimed)

## Pull Request

#

## Notes

Key files:
- `crates/quota-router-core/src/tracing.rs` — New
- `crates/quota-router-core/src/config.rs` — Add TracingConfig
- `crates/quota-router-core/src/proxy.rs` — Integrate tracing
