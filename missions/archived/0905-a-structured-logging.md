# Mission: 0905-a — Structured Logging

> **Path B closure:** AC + code verified 2026-07-30 via mission audit. Code ground: `crates/quota-router-core/src/proxy.rs:243` (`if let Some(ref lg) = logger { ... }` block emitting `LogEvent` via `StructuredLogger::request_completed` / `request_failed`), `proxy.rs:250-261` `with_logger` builder, `proxy.rs:386-411` `rfc0905_tests::with_logger_attaches_handle` tokio test. All 11 ACs now checked. Did not pass through `claimed/` or `with-pr/` — work landed in `next` via prior PRs whose PR trail is lost to audit. Mission predates RFC-0944 (`tracing::info!` at proxy.rs:2239) — the two coexist: tracing emits span-based human-readable logs (RFC-0944 contract), StructuredLogger emits NDJSON `LogEvent` records (RFC-0905 contract).

## Status

Completed (Archived 2026-07-30 — Path B)

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
- [x] Integrate structured logging into `proxy.rs` for request/response events (RFC-0905 NDJSON side-channel; coexists with RFC-0944 `tracing::info!` at proxy.rs:2239)
- [x] Never-log list: Authorization, X-API-Key, Cookie headers
- [x] Clippy passes with zero warnings
- [x] All existing tests pass

## Claimant

(unclaimed)

## Pull Request

#

## Notes

Key files:
- `crates/quota-router-core/src/logging.rs` — New (`LogLevel`, `LogEvent`, `LogConfig`, `StructuredLogger`)
- `crates/quota-router-core/src/config.rs:817` — `LogConfig` struct (level, format, sample_rate, buffer_size, flush_interval_ms, always_log_errors)
- `crates/quota-router-core/src/proxy.rs` — Integrate logging
  - `:212` `ProxyServer.logger: Option<Arc<StructuredLogger>>`
  - `:250` `with_logger` builder
  - `:243` per-request emit (`request_completed` on 2xx, `request_failed` on non-2xx/err)
  - `:386-411` `rfc0905_tests::with_logger_attaches_handle` tokio test
