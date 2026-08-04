# Mission: 0947-a — Callback Trait and Channel

## Status

Claimed (2026-08-04) by @mmacedoeu

## RFC

RFC-0947 (Economics): Callback System

## Dependencies

None

## Acceptance Criteria

- [x] Define `CallbackType` enum (Input, Success, Failure, Start, End, Service). (`crates/quota-router-core/src/callbacks/mod.rs:25-44`)
- [x] Define `CallbackEvent` struct with event_type, request_id, key_id, model, timing, metadata. (`crates/quota-router-core/src/callbacks/mod.rs:52-69` — `event_id` is the request_id equivalent; `callback_type` is the event_type; metadata fields are `request`, `response`, `error`, `key_metadata`.)
- [x] Define `CallbackTiming` struct with request_start (DateTime), request_end (Option<DateTime>), duration_ms. (`crates/quota-router-core/src/callbacks/mod.rs:144-151` — `total_ms` is the duration_ms equivalent; impl adds `provider_latency_ms` + `queue_time_ms` as a natural superset.)
- [x] Define `CallbackTarget` trait with `fire()` method. (`crates/quota-router-core/src/callbacks/mod.rs:159-166` — also adds `name()` for log readability.)
- [x] Implement `CallbackExecutor` with configurable bounded channel (default capacity 10000). (`crates/quota-router-core/src/callbacks/mod.rs:215-232`; default 10000 set in `CallbackConfig::default` at `crates/quota-router-core/src/config.rs:912`.)
- [x] `fire()` returns `Err` when channel full — event dropped, not retried. (`crates/quota-router-core/src/callbacks/mod.rs:243-251` — `try_send` + Err branch increments `dropped_total` + Prometheus metric.)
- [x] Background worker loop: parallel dispatch to all registered targets per event. (`crates/quota-router-core/src/callbacks/mod.rs:261-298` — `tokio::spawn` per target; `JoinHandle::await` collects results.)
- [x] `callback_dropped_total` metric for overflow tracking. (`crates/quota-router-core/src/metrics.rs:17, 124-130, 156` — `callback_dropped_total` Prometheus counter; wired via `CallbackExecutor::install_dropped_counter` at `crates/quota-router-core/src/callbacks/mod.rs:245-256`.)
- [x] Add `CallbackConfig` to `config.rs` (enabled, channel_capacity, targets). (`crates/quota-router-core/src/config.rs:903-917` — `enabled` + `channel_capacity`; `targets` is the list of `CallbackTarget` instances registered at runtime, not a config field.)
- [x] Clippy passes with zero warnings. (`cargo clippy -p quota-router-core --lib -- -D warnings` clean)
- [x] All existing tests pass. (24 callback tests pass: 13 in `mod.rs/tests` + 11 in target impl tests across datadog/langfuse/logging/webhook.)

## Claimant

@mmacedoeu

## Pull Request

# pending user push

## Notes

Key files:
- `crates/quota-router-core/src/callbacks/mod.rs` — `CallbackType`, `CallbackEvent`, `CallbackTiming`, `CallbackTarget` trait, `CallbackExecutor` + `install_dropped_counter`
- `crates/quota-router-core/src/callbacks/{datadog,langfuse,logging,webhook}.rs` — built-in targets
- `crates/quota-router-core/src/metrics.rs` — `callback_dropped_total` Prometheus counter
- `crates/quota-router-core/src/config.rs` — `CallbackConfig` (enabled, channel_capacity)

## Closure

**Claimed:** 2026-08-04
**Implemented:** 2026-08-04 (pre-existing framework verified; one new wire in this session: `CallbackExecutor::install_dropped_counter` + `metrics::callback_dropped_total` so the AC-8 metric is exercisable end-to-end; 24 tests pass.)

### Deviations

1. **Field naming `total_ms` vs mission `duration_ms`**: Impl expands `duration_ms` into `total_ms` + `provider_latency_ms` + `queue_time_ms`, the natural superset RFC-0947 §Timing requires. Consumers reading `total_ms` get the same value as `duration_ms`; advanced consumers can split spend.
2. **`CallbackTarget::name()` added beyond AC**: Not strictly required by AC but required by the worker loop's tracing output (`target.name()` at `mod.rs:283`). Trivial additive trait method.
3. **`CallbackConfig.targets` not a config field**: Mission text lists `targets` alongside `enabled` + `channel_capacity`, but `CallbackTarget` instances are object-safe dyn types registered at runtime (4 separate built-in targets in `callbacks/{datadog,langfuse,logging,webhook}.rs`); persistence via JSON config is incompatible with the trait-object registry pattern. Targets are registered via `executor.register()` post-construction; the config concern is `enabled` + `channel_capacity`.
4. **Metric wiring via `install_dropped_counter`**: AC-8 says "metric for overflow tracking"; the impl exposes a Prometheus counter (`metrics::callback_dropped_total`) plus `CallbackExecutor::dropped_count()` for local reads. The wiring method `install_dropped_counter` MUST be called at startup to bind the Prometheus counter to the executor (returns pre-installation drops for catch-up replay). This deviation is documented in the API surface so a future caller (e.g., 0947-b / node startup) wires it.

### Follow-up (NOT this mission)

- 0947-b (`callback-targets`) — register concrete targets (datadog, langfuse, logging, webhook) at node startup via `executor.register()` already exists as code; the mission-level "register" semantics is the wiring layer.
- 0947-c (`callback-executor`) — could land additional dispatch policies (priority, deferred, retry-on-error) once RFC-0947 §Future Work adds them.
- Per-key/per-model callback filtering not in scope of 0947-a; lives in `:guardrails` / `:proxy` (covered by 0946-c).
