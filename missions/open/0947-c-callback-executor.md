# Mission: 0947-c — Callback Executor

## Status

Open

## RFC

RFC-0947 (Economics): Callback System

## Dependencies

- Mission-0947-a: Callback Trait and Channel
- Mission-0947-b: Callback Targets

## Acceptance Criteria

- [ ] Wire `CallbackExecutor` into `proxy.rs` — fire Start callback after key validation, fire End/Success/Failure after provider response
- [ ] Streaming callback semantics: fire once per request (not per chunk)
- [ ] Input callback: fire before provider call (supports input validation/transformation)
- [ ] Service callback: fire for health/monitoring events
- [ ] Add `input_callback`, `success_callback`, `failure_callback`, `service_callback` to Python SDK
- [ ] Support custom callback functions via PyO3
- [ ] Match LiteLLM callback interface
- [ ] Clippy passes with zero warnings
- [ ] All existing tests pass

## Claimant

(unclaimed)

## Pull Request

#

## Notes

Key files:
- `crates/quota-router-core/src/callbacks/executor.rs` — Wire into proxy
- `crates/quota-router-core/src/proxy.rs` — Fire callbacks
- `crates/quota-router-core/src/python_sdk/mod.rs` — Python callback support
