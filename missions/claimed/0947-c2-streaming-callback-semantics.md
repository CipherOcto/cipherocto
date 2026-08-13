# Mission: 0947-c2 — Streaming Callback Semantics

## Status

LANDED 2026-08-13. Follow-on to `0947-c-callback-executor` (CLOSED 2026-08-13, DEFERRED pattern). Once-per-request streaming callback semantics landed at `crates/quota-router-core/src/callbacks/mod.rs` + `config.rs` + `proxy.rs::handle_streaming` (via `StreamingCallbackWiring`).

## RFC

RFC-0947 (Economics): Callback System §Streaming Semantics

## Dependencies

- Mission-0947-c1: Proxy End/Success/Failure Wiring (must land first to fire End at streaming completion)

## Acceptance Criteria

- [x] Document streaming callback semantics in `crates/quota-router-core/src/callbacks/mod.rs` module-level doc — LANDED (header table + breaking-change note)
- [x] Define `StreamingCallbackPolicy { OncePerRequest, PerChunk, Disabled }` enum — LANDED (callbacks/mod.rs:41 with `#[default] OncePerRequest`)
- [x] Default policy: `OncePerRequest` (matches LiteLLM) — LANDED (`#[derive(Default)]` with `#[default] OncePerRequest`)
- [x] Wire policy lookup into proxy.rs streaming path (SSE / chunked transfer) — LANDED (proxy.rs `handle_streaming` + `StreamingCallbackWiring` struct)
- [x] End callback fires at stream completion (not first chunk arrival) — LANDED (Start fires once at entry via `fire_streaming_start`; End fires at completion via `fire_end_success`/`fire_end_failure` from mission 0947-c1)
- [x] Success/Failure fires at stream completion based on terminal status (NOT intermediate chunk errors) — LANDED (terminal-status branch in chunk-forwarding task; intermediate errors break the loop and trigger Failure via the same path)
- [x] Add `streaming_callback_policy` config option to `QuotaRouterConfig` — LANDED (config.rs `CallbackConfig.streaming_callback_policy`)
- [x] Add at least 3 tests — LANDED (5 new tests: default, serde, start event, start emit, config round-trip)
- [x] Document breaking-change note for consumers who relied on per-chunk firing in LiteLLM — LANDED (module-level doc + breaking-change paragraph)
- [x] Clippy passes with zero warnings — LANDED (`-D warnings` clean)
- [x] All existing tests pass + new streaming-semantics tests (≥3) — LANDED (1718 lib tests pass; +5 new streaming tests)

## Claimant

Implementation agent (claim 2026-08-13).

## Pull Request

#

## Notes

Key files:

- `crates/quota-router-core/src/callbacks/mod.rs` — add `StreamingCallbackPolicy` enum + module docs
- `crates/quota-router-core/src/proxy.rs` — streaming path (no `streaming.rs` exists yet; locate SSE / chunked handling inline)
- `crates/quota-router-core/src/config.rs` — add `streaming_callback_policy` field

Reference points:

- `proxy.rs:659` — request handler has streaming path (find via `Content-Type: text/event-stream` / chunked handling)
- RFC-0947 §Streaming Semantics — design rationale for once-per-request vs per-chunk

Coupling:

- Mission 0947-c1 must land first (End fire at stream completion is the implementation point)
- Mission 0947-c3 (PyO3 SDK) consumes this policy via config

## Version History

| Version | Date       | Change                                                                                                        |
| ------- | ---------- | ------------------------------------------------------------------------------------------------------------- |
| v0.1    | 2026-08-13 | Mission filed. Follow-on to `0947-c-callback-executor` closure. Streaming once-per-request semantics. 11 ACs. |

Last Updated: 2026-08-13
Version: 0.1
