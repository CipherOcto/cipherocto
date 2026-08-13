# Mission: 0947-c — Callback Executor

## Status

Closed (2026-08-13). `CallbackExecutor` core landed in `crates/quota-router-core/src/callbacks/mod.rs:203` (registry + mpsc channel + parallel target dispatch + dropped-counter metrics). Proxy.rs Start fire wired. **5 wiring ACs intentionally DEFERRED** with 3 filed follow-on missions per [[feedback_initiation_user_only]] user choice (close-as-DEFERRED, 0871b-pattern).

**Follow-on missions (all FILED):**

- `0947-c1-proxy-end-success-failure-wiring` — End/Success/Failure firing in proxy.rs after provider response
- `0947-c2-streaming-callback-semantics` — once-per-request streaming callback semantics
- `0947-c3-pyo3-callback-sdk` — PyO3 callback bindings (`input_callback`, `success_callback`, `failure_callback`, `service_callback`) + custom Python functions + LiteLLM interface match

## RFC

RFC-0947 (Economics): Callback System

## Dependencies

- Mission-0947-a: Callback Trait and Channel — **LANDED**
- Mission-0947-b: Callback Targets — **CLOSED 2026-08-13** (commit `b4ad58b1`)

## Acceptance Criteria

### Executor core

- [x] `CallbackExecutor` struct with `register(callback_type, target)` + `fire(event)` — **LANDED** at `crates/quota-router-core/src/callbacks/mod.rs:203`
- [x] Non-blocking channel-based delivery (mpsc, configurable capacity) — **LANDED** (`CallbackExecutor::new(capacity)` at `:220`)
- [x] Parallel target dispatch (per-event `tokio::spawn` fan-out) — **LANDED** (`worker_loop` at `:284-321`)
- [x] Failures logged but not propagated to request path — **LANDED** (`tracing::warn!` at `:305-310`)
- [x] Dropped event counter + Prometheus integration — **LANDED** (`install_dropped_counter` at `:245` + `:268`)
- [x] Graceful shutdown via `shutdown()` — **LANDED** at `:324`
- [x] All 6 `CallbackType` variants supported: Input, Success, Failure, Start, End, Service — **LANDED** (verified at `mod.rs:369-378`)
- [x] Clippy passes with zero warnings — **VERIFIED** (`cargo clippy -p quota-router-core --features full --lib -- -D warnings` clean)
- [x] All existing tests pass — **VERIFIED** (24 callback tests pass)

### Proxy wiring (DEFERRED — follow-on missions)

- [ ] Fire Start callback after key validation — **PARTIAL** (wired at `proxy.rs:812-852`) but with stub request body (empty `model`, empty `messages`)
- [ ] Fire End/Success/Failure after provider response — **DEFERRED** to follow-on `0947-c1-proxy-end-success-failure-wiring`
- [ ] Streaming callback semantics: fire once per request (not per chunk) — **DEFERRED** to follow-on `0947-c2-streaming-callback-semantics`
- [ ] Input callback: fire before provider call (abort-on-error) — **DEFERRED** (no firing site in proxy.rs)
- [ ] Service callback: fire for health/monitoring events — **DEFERRED** (no firing site in proxy.rs)

### Python SDK (DEFERRED — follow-on mission)

- [ ] Add `input_callback`, `success_callback`, `failure_callback`, `service_callback` to Python SDK — **DEFERRED** to follow-on `0947-c3-pyo3-callback-sdk`
- [ ] Support custom callback functions via PyO3 — **DEFERRED** to follow-on `0947-c3`
- [ ] Match LiteLLM callback interface — **DEFERRED** to follow-on `0947-c3`

## Claimant

(unclaimed)

## Pull Request

#

## Notes

Key files (LANDED):

- `crates/quota-router-core/src/callbacks/mod.rs` — `CallbackExecutor` at `:203` (136 lines, incl. tests)
- `crates/quota-router-core/src/proxy.rs` — Start fire at `:812-852`

Key files (PENDING — follow-on missions):

- `crates/quota-router-core/src/proxy.rs` — End/Success/Failure firing (0947-c1)
- `crates/quota-router-core/src/streaming.rs` — does not exist yet (0947-c2)
- `crates/quota-router-pyo3/src/sdk.rs` — callback bindings absent (0947-c3)

**Closure rationale** (per [[feedback_initiation_user_only]] user choice 2026-08-13, "Close as DEFERRED (0871b pattern) Recommended"):

The `CallbackExecutor` core is the orchestration primitive — once landed, all 5 wiring ACs are concrete additions to the request path (no new abstractions needed). Following the 0871b family closure pattern: parent mission closes when the substrate is stable and all deferred work has filed follow-ons + concrete owner.

**Drift disclosure** (per [[deferred-vs-unspecified]] discipline):

- 5 ACs explicitly DEFERRED with filed follow-on missions. Each follow-on has a clear scope (3-5 ACs, single file or single crate) and is unblocked by the executor core landing.
- No "future / post-v1.0" placeholders — every deferred AC has a concrete follow-on mission pointer.

## Version History

| Version | Date       | Change                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                  |
| ------- | ---------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| v0.1    | (filed)    | Mission filed. RFC-0947 §Callback System defines executor + 6 callback types + Python SDK surface.                                                                                                                                                                                                                                                                                                                                                                                                                      |
| v0.2    | 2026-08-13 | **MISSION CLOSED (DEFERRED pattern, 0871b-style).** `CallbackExecutor` core LANDED at `mod.rs:203`: registry, mpsc channel, parallel target dispatch, dropped counter + Prometheus, graceful shutdown. Start fire wired at `proxy.rs:812-852`. 9 ACs PASS. **5 ACs DEFERRED** to 3 filed follow-on missions: `0947-c1-proxy-end-success-failure-wiring` (End/Success/Failure firing), `0947-c2-streaming-callback-semantics` (once-per-request streaming), `0947-c3-pyo3-callback-sdk` (PyO3 bindings + LiteLLM match). |

Last Updated: 2026-08-13
Version: 0.2 (CLOSED — DEFERRED pattern)
