# Mission: 0944-a — Structured JSON Logging

## Status

Open

## RFC

RFC-0944 (Economics): Observability & Logging Backends

> **Numbering note.** RFC-0944 was never authored. Git history contains no
> RFC file with this number under any category, so this mission was filed
> against a specification that does not exist. The closest current RFC by title is RFC-0905 (Economics): Observability and Logging at `rfcs/accepted/economics/0905-observability-logging.md`, which this
> mission did not cite and which is not recorded as a renumbering of
> RFC-0944. Treat the governing specification as unconfirmed and
> reconcile it against RFC-0905 (Economics): Observability and Logging before relying on it.

## Context

The current logging uses tracing with default formatting. This mission adds structured JSON output for production use.

## Acceptance Criteria

- [ ] Add JSON logging option via config
- [ ] Include request_id, model, provider, duration, status in log entries
- [ ] PII redaction for API keys in logs
- [ ] Feature flag for JSON vs text logging

## Files to Modify

- `crates/quota-router-core/src/proxy.rs` — add structured logging
