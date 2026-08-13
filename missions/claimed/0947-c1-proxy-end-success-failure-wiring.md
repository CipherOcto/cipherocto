# Mission: 0947-c1 — Proxy End/Success/Failure Wiring

## Status

LANDED 2026-08-13. Follow-on to `0947-c-callback-executor` (CLOSED 2026-08-13, DEFERRED pattern). `CallbackExecutor` core landed at `crates/quota-router-core/src/callbacks/mod.rs:203`; Start fire wired at `proxy.rs:812-852`. This mission wires the remaining 3 callback types into the request path.

**Landing scope:** 3 builder functions (`build_end_event` / `build_success_event` / `build_failure_event`) + 2 async helpers (`fire_end_success` / `fire_end_failure`) in `callbacks/mod.rs`. End/Success/Failure fires wired in `proxy.rs` `handle_request` at the final `result` return point (after rate-limit header injection block). 6 new unit tests added (all 30 callback tests pass; 228 proxy tests pass). Clippy `-D warnings` clean.

## RFC

RFC-0947 (Economics): Callback System

## Dependencies

- Mission-0947-c: Callback Executor (CLOSED 2026-08-13)

## Acceptance Criteria

- [x] Fire `End` callback after provider response (success or failure — final marker) — **LANDED** at `proxy.rs` post-rate-limit-headers block via `fire_end_success` / `fire_end_failure`
- [x] Fire `Success` callback after provider success response (with response_summary, usage, latency) — **LANDED** via `build_success_event` carrying `CallbackResponse` payload
- [x] Fire `Failure` callback after provider error or local error (with error code + reason) — **LANDED** via `build_failure_event` carrying `CallbackErrorDetail` payload (`error_type` + `message` + `status_code` + `provider`)
- [x] Response data passed to callback: `CallbackResponse { id, model, response_summary, usage, latency_ms, provider, cached }` — **LANDED** (proxy wires `CallbackResponse` with all 7 fields populated from `handle_request` context)
- [x] Error data passed to callback: `CallbackError { code, message, provider }` with provider source — **LANDED** (`CallbackErrorDetail` populates `error_type` for HTTP errors vs `internal_proxy_error`)
- [ ] Streaming + non-streaming paths both fire End/Success/Failure (one fire per request, not per chunk — see 0947-c2 for chunk-skip semantics) — **PARTIAL** (main `/v1/chat/completions` dispatch fires; streaming-chunk semantics are 0947-c2 scope)
- [x] All callback fires occur AFTER response/error sent to client (no request-path latency penalty from callback delivery — async via mpsc) — **LANDED** (`fire()` uses `try_send` on bounded channel; fire ordering: Success/Failure first, then End, after `result` is built)
- [x] Clippy passes with zero warnings — **VERIFIED** (`cargo clippy -p quota-router-core --all-targets --features full -- -D warnings` clean)
- [x] All existing tests pass + new wiring tests (≥4: end, success, failure, both-paths) — **VERIFIED** (6 new unit tests; all 30 callback tests pass; 228 proxy tests pass)

## Claimant

(unclaimed)

## Pull Request

#

## Notes

Key files:

- `crates/quota-router-core/src/proxy.rs` — 11684 lines; locate existing Start fire at `:812-852` and add End/Success/Failure firing at the provider-response + error branches
- `crates/quota-router-core/src/callbacks/mod.rs` — `CallbackExecutor::fire` at `:263`

Reference points:

- `proxy.rs:659` — `callback_executor: Option<Arc<CallbackExecutor>>` field on request handler
- `proxy.rs:813` — Start fire pattern (clone + build event + `fire().await`)

Drift inheritance: none — this is greenfield wiring work, not closing existing drift.

## Version History

| Version | Date       | Change                                                                                                                                   |
| ------- | ---------- | ---------------------------------------------------------------------------------------------------------------------------------------- |
| v0.1    | 2026-08-13 | Mission filed. Follow-on to `0947-c-callback-executor` closure. End/Success/Failure firing into proxy.rs after provider response. 9 ACs. |
| v0.2    | 2026-08-13 | **LANDED.** 3 builders + 2 async helpers in `callbacks/mod.rs` (`build_end_event` / `build_success_event` / `build_failure_event` / `fire_end_success` / `fire_end_failure`). End/Success/Failure wired in `proxy.rs handle_request` at final `result` return. 6 new unit tests; 30 callback + 228 proxy tests pass. Clippy `-D warnings` clean. AC-6 streaming-chunk-skip semantics PARTIAL (deferred to 0947-c2). |

Last Updated: 2026-08-13
Version: 0.2 (LANDED)
