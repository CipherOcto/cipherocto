# Mission: 0905-a — Structured Logging

## Status

Completed

## RFC

RFC-0905 (Economics): Observability and Logging

## Dependencies

None

## Acceptance Criteria

- [x] Define `LogLevel` enum (Debug, Info, Warn, Error)
- [x] Define `LogEvent` struct with timestamp, level, component, event, trace_id, request_id
- [x] Implement NDJSON serialization (one JSON object per line)
- [x] Implement PII redaction rules (API keys → `[REDACTED]`, emails → `[EMAIL_REDACTED]`, etc.)
- [x] Implement async buffered writer with configurable buffer size and flush interval
- [x] Implement log sampling with configurable `sample_rate`
- [x] Add `LogConfig` to `config.rs` (level, format, sample_rate, buffer_size, flush_interval_ms)
- [ ] Integrate structured logging into `proxy.rs` for request/response events
- [x] Never-log list: Authorization, X-API-Key, Cookie headers
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
