# Mission: 0947-b — Callback Targets

## Status

Open

## RFC

RFC-0947 (Economics): Callback System

## Dependencies

- Mission-0947-a: Callback Trait and Channel

## Acceptance Criteria

- [ ] Implement `LangfuseTarget` — HTTP API integration
- [ ] Implement `DatadogTarget` — HTTP API integration
- [ ] Implement `WebhookTarget` — generic HTTP POST with HMAC-SHA256 signing
- [ ] HMAC uses `hmac` + `sha2` crates (not `ring`)
- [ ] Implement `LoggingTarget` — integration with RFC-0905 structured logging
- [ ] Each target has configurable timeout and retry policy
- [ ] Per-target retry with exponential backoff (max 3 attempts)
- [ ] ResponseSummary used instead of full response content (no PII leakage)
- [ ] Clippy passes with zero warnings
- [ ] All existing tests pass

## Claimant

(unclaimed)

## Pull Request

#

## Notes

Key files:
- `crates/quota-router-core/src/callbacks/langfuse.rs` — New
- `crates/quota-router-core/src/callbacks/datadog.rs` — New
- `crates/quota-router-core/src/callbacks/webhook.rs` — New
- `crates/quota-router-core/src/callbacks/logging.rs` — New
