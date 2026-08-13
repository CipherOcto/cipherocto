# Mission: 0947-c2 — Streaming Callback Semantics

## Status

Open. Follow-on to `0947-c-callback-executor` (CLOSED 2026-08-13, DEFERRED pattern). Defines once-per-request streaming callback semantics — critical for SSE / chunked responses where naive per-chunk firing would flood the callback channel.

## RFC

RFC-0947 (Economics): Callback System §Streaming Semantics

## Dependencies

- Mission-0947-c1: Proxy End/Success/Failure Wiring (must land first to fire End at streaming completion)

## Acceptance Criteria

- [ ] Document streaming callback semantics in `crates/quota-router-core/src/callbacks/mod.rs` module-level doc
- [ ] Define `StreamingCallbackPolicy { OncePerRequest, PerChunk, Disabled }` enum
- [ ] Default policy: `OncePerRequest` (matches LiteLLM)
- [ ] Wire policy lookup into proxy.rs streaming path (SSE / chunked transfer)
- [ ] End callback fires at stream completion (not first chunk arrival)
- [ ] Success/Failure fires at stream completion based on terminal status (NOT intermediate chunk errors)
- [ ] Add `streaming_callback_policy` config option to `QuotaRouterConfig`
- [ ] Add at least 3 tests: once-per-request fires End at completion (not chunk arrival); per-chunk disabled by default; per-chunk policy fires per chunk for backward compat
- [ ] Document breaking-change note for consumers who relied on per-chunk firing in LiteLLM
- [ ] Clippy passes with zero warnings
- [ ] All existing tests pass + new streaming-semantics tests (≥3)

## Claimant

(unclaimed)

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
