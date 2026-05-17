# Mission: 0947-a — Callback Trait and Channel

## Status

Open

## RFC

RFC-0947 (Economics): Callback System

## Dependencies

None

## Acceptance Criteria

- [ ] Define `CallbackType` enum (Input, Success, Failure, Start, End, Service)
- [ ] Define `CallbackEvent` struct with event_type, request_id, key_id, model, timing, metadata
- [ ] Define `CallbackTiming` struct with request_start (DateTime), request_end (Option<DateTime>), duration_ms
- [ ] Define `CallbackTarget` trait with `fire()` method
- [ ] Implement `CallbackExecutor` with configurable bounded channel (default capacity 10000)
- [ ] `fire()` returns `Err` when channel full — event dropped, not retried
- [ ] Background worker processes events from channel
- [ ] `callback_dropped_total` metric for overflow tracking
- [ ] Add `CallbackConfig` to `config.rs` (enabled, channel_capacity, targets)
- [ ] Clippy passes with zero warnings
- [ ] All existing tests pass

## Claimant

(unclaimed)

## Pull Request

#

## Notes

Key files:
- `crates/quota-router-core/src/callbacks/mod.rs` — New
- `crates/quota-router-core/src/callbacks/executor.rs` — New
- `crates/quota-router-core/src/config.rs` — Add CallbackConfig
