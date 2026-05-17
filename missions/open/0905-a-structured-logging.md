# Mission: 0905-a — Structured Logging

## Status

Open

## RFC

RFC-0905 (Economics): Observability and Logging

## Dependencies

None

## Acceptance Criteria

- [ ] Define `LogLevel` enum (Debug, Info, Warn, Error)
- [ ] Define `LogEvent` struct with timestamp, level, component, event, trace_id, request_id
- [ ] Implement NDJSON serialization (one JSON object per line)
- [ ] Implement PII redaction rules (API keys → `[REDACTED]`, emails → `[EMAIL_REDACTED]`, etc.)
- [ ] Implement async buffered writer with configurable buffer size and flush interval
- [ ] Implement log sampling with configurable `sample_rate`
- [ ] Add `LogConfig` to `config.rs` (level, format, sample_rate, buffer_size, flush_interval_ms)
- [ ] Integrate structured logging into `proxy.rs` for request/response events
- [ ] Never-log list: Authorization, X-API-Key, Cookie headers
- [ ] Clippy passes with zero warnings
- [ ] All existing tests pass

## Claimant

(unclaimed)

## Pull Request

#

## Notes

Key files:
- `crates/quota-router-core/src/logging.rs` — New
- `crates/quota-router-core/src/config.rs` — Add LogConfig
- `crates/quota-router-core/src/proxy.rs` — Integrate logging
