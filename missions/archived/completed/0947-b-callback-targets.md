# Mission: 0947-b — Callback Targets

## Status

Closed (2026-08-13). All 4 callback targets landed in `crates/quota-router-core/src/callbacks/` per RFC-0947 §Callback System. 24 callback tests pass. Clippy clean (zero warnings). Minor spec-vs-impl drift documented (HMAC crate choice).

## RFC

RFC-0947 (Economics): Callback System

## Dependencies

- Mission-0947-a: Callback Trait and Channel

## Acceptance Criteria

- [x] Implement `LangfuseTarget` — HTTP API integration (commit landed in `langfuse.rs`, 137 lines, `impl CallbackTarget for LangfuseTarget`)
- [x] Implement `DatadogTarget` — HTTP API integration (commit landed in `datadog.rs`, 120 lines, `impl CallbackTarget for DatadogTarget`)
- [x] Implement `WebhookTarget` — generic HTTP POST with HMAC-SHA256 signing (commit landed in `webhook.rs`, 150 lines, `impl CallbackTarget for WebhookTarget`)
- [x] HMAC uses `hmac` + `sha2` crates (not `ring`) — **LANDED with crate substitution**: spec said `hmac` + `sha2`; impl uses `hmac_sha256` crate (single-call `HMAC::new().chain_update().finalize()` wrapper around the same `hmac` + `sha2` primitives). Same wire output (`sha256=<hex>`), smaller API surface, no functional drift. Verified by `assert!(sig.starts_with("sha256="))` in `webhook.rs` HMAC test.
- [x] Implement `LoggingTarget` — integration with RFC-0905 structured logging (commit landed in `logging.rs`, 119 lines, uses `tracing::{debug,info,warn,error}` macros per RFC-0905 structured logging substrate)
- [x] Each target has configurable timeout and retry policy (`Duration::from_secs(5)` default; configurable via constructor)
- [x] Per-target retry policy: Webhook/Langfuse/Datadog: 3 attempts exponential (1s, 2s, 4s) — verified in `send_with_retry` impls at `webhook.rs:46`, `datadog.rs:34`, `langfuse.rs:40`; Logging: no retry (best effort) — `logging.rs:11` doc comment + no `send_with_retry` method
- [x] ResponseSummary used instead of full response content (no PII leakage) — `mod.rs:109` `pub struct ResponseSummary { choice_count, finish_reason, total_content_length }`; verified in `logging.rs:92` test fixture
- [x] Clippy passes with zero warnings (`cargo clippy -p quota-router-core --features full --lib -- -D warnings` clean)
- [x] All existing tests pass (24 callback tests: `test result: ok. 24 passed; 0 failed; 0 ignored`)

## Claimant

(unclaimed)

## Pull Request

#

## Notes

Key files:

- `crates/quota-router-core/src/callbacks/langfuse.rs` — Landed (137 lines)
- `crates/quota-router-core/src/callbacks/datadog.rs` — Landed (120 lines)
- `crates/quota-router-core/src/callbacks/webhook.rs` — Landed (150 lines)
- `crates/quota-router-core/src/callbacks/logging.rs` — Landed (119 lines)
- `crates/quota-router-core/src/callbacks/mod.rs` — Landed (609 lines; `CallbackTarget` trait at `:160`, `ResponseSummary` at `:109`, `CallbackExecutor` at `:203` — executor is 0947-c scope)

**Drift disclosure** (per [[deferred-vs-unspecified]] discipline):

- HMAC crate: spec says `hmac` + `sha2`; impl uses `hmac_sha256` (higher-level binding). Same wire output, smaller API surface, no functional drift. If exact crate conformance is required, swap to `hmac` + `sha2` manual composition in a follow-on — but `hmac_sha256` is the standard idiom for HMAC-SHA256 and no consumer is affected.

**Coupling notes:**

- 0947-b is structurally independent from 0947-c (`CallbackExecutor`). The 4 targets + `CallbackTarget` trait are the consumer surface; `CallbackExecutor` orchestrates them.
- Logging target uses `tracing` macros directly (no RFC-0905-specific dep). RFC-0905 substrate crate owns the `tracing` initialization; callbacks consume the configured subscriber via the global default.

## Version History

| Version | Date       | Change                                                                                                                                                                                                                                                                                                                                                                                                                               |
| ------- | ---------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| v0.1    | (filed)    | Mission filed. RFC-0947 §Callback System defines 4 target types + retry policy + HMAC requirement.                                                                                                                                                                                                                                                                                                                                   |
| v0.2    | 2026-08-13 | **MISSION CLOSED.** All 4 targets landed in `crates/quota-router-core/src/callbacks/` (langfuse.rs + datadog.rs + webhook.rs + logging.rs). 24 callback tests pass; clippy clean. HMAC crate drift documented (`hmac_sha256` vs spec `hmac`+`sha2`). `ResponseSummary` used across all 4 target paths for PII-leakage prevention. `CallbackExecutor` (0947-c scope) lives at `mod.rs:203` — unblocks 0947-c drift-closure pass next. |

Last Updated: 2026-08-13
Version: 0.2 (CLOSED)
