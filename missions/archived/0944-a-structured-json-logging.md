# Mission: 0944-a — Structured JSON Logging

## Status

Open

## RFC

RFC-0944 (Economics): Observability & Logging Backends

## Context

The current logging uses tracing with default formatting. This mission adds structured JSON output for production use.

## Acceptance Criteria

- [ ] Add JSON logging option via config
- [ ] Include request_id, model, provider, duration, status in log entries
- [ ] PII redaction for API keys in logs
- [ ] Feature flag for JSON vs text logging

## Files to Modify

- `crates/quota-router-core/src/proxy.rs` — add structured logging
