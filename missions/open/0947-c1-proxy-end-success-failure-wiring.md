# Mission: 0947-c1 — Proxy End/Success/Failure Wiring

## Status

Open. Follow-on to `0947-c-callback-executor` (CLOSED 2026-08-13, DEFERRED pattern). `CallbackExecutor` core landed at `crates/quota-router-core/src/callbacks/mod.rs:203`; Start fire wired at `proxy.rs:812-852`. This mission wires the remaining 3 callback types into the request path.

## RFC

RFC-0947 (Economics): Callback System

## Dependencies

- Mission-0947-c: Callback Executor (CLOSED 2026-08-13)

## Acceptance Criteria

- [ ] Fire `End` callback after provider response (success or failure — final marker)
- [ ] Fire `Success` callback after provider success response (with response_summary, usage, latency)
- [ ] Fire `Failure` callback after provider error or local error (with error code + reason)
- [ ] Response data passed to callback: `CallbackResponse { id, model, response_summary, usage, latency_ms, provider, cached }`
- [ ] Error data passed to callback: `CallbackError { code, message, provider }` with provider source
- [ ] Streaming + non-streaming paths both fire End/Success/Failure (one fire per request, not per chunk — see 0947-c2 for chunk-skip semantics)
- [ ] All callback fires occur AFTER response/error sent to client (no request-path latency penalty from callback delivery — async via mpsc)
- [ ] Clippy passes with zero warnings
- [ ] All existing tests pass + new wiring tests (≥4: end, success, failure, both-paths)

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

Last Updated: 2026-08-13
Version: 0.1
