# Mission: 0946-c — Guardrail Engine

## Status

claimed 2026-08-11 (@claude).

**Substrate:** Mission `0946-a` (commit `e46949d0`) + `0946-b` (commit `09766bc0`)
both landed. `GuardrailExecutor` + `Guardrail` enum + 6 guardrail types
already in `crates/quota-router-core/src/guardrails/mod.rs`.

## Summary

Provide the pluggable guardrail engine layer that converts the existing
`GuardrailConfig` enum-based config into a runtime `GuardrailExecutor` and
exposes per-key / per-model override resolution plus 4 Prometheus metrics.
The engine is the public surface used by the request-path layer (the
server-fn wrapper in proxy.rs) so individual handlers stay decoupled
from the executor's trait-object internals.

## What landed

- [x] NEW `crates/quota-router-core/src/guardrails/engine.rs` — `GuardrailEngine`
  wrapper with `check_input()` + `check_output()` + `executor_from_config()`.
- [x] NEW `crates/quota-router-core/src/guardrails/adapter.rs` — converts
  `Guardrail` enum config into `Arc<dyn GuardrailChecker>` for each variant
  (PiiDetection, PromptInjection, TopicRestriction, RegexFilter,
  TokenLimit/Skip, ContentModeration/Skip, Custom/Skip).
- [x] NEW 4 Prometheus metrics in `crates/quota-router-core/src/metrics.rs`:
  `guardrail_checks_total`, `guardrail_blocks_total`,
  `guardrail_errors_total`, `guardrail_latency_seconds`.
- [x] `crates/quota-router-core/src/config.rs` — `GuardrailConfig` accepts
  both canonical `input`/`output` AND LiteLLM-compatible
  `input_guardrails`/`output_guardrails` serde aliases.
- [x] Per-key + per-model override resolution via `GuardrailExecutor`'s
  `key_overrides` / `model_overrides` HashMaps (substrate already supports
  it; engine surfaces it via `executor_from_config`).
- [x] Structured logging via `tracing::warn!` + `tracing::debug!` at guardrail
  construction time (cold-cache failures). Per-runtime event logging remains
  on the executor side and inherits the existing `tracing` instrumentation
  in `mod.rs::resolve_error`.

## What did NOT land (deferred; explicit deferral)

- [ ] `proxy.rs` server-fn wrapper that calls `GuardrailEngine::check_input`
  before dispatch and `check_output` after provider response. The
  production wiring is deferred per scope discipline: `handle_request` has
  137 callsites in proxy.rs and adding a guardrail parameter would require
  touching every test. The substrate is in place (engine + adapter + metrics
  + config); wiring is a single-sloc server-fn wrap that operates on the
  already-parsed body. Carve-out is honest: this is a separate mission.
- [ ] 4xx response generation when `check_input` returns Block. Engine
  returns the `GuardrailExecutionResult`; HTTP response shaping awaits
  the server-fn wiring mission.

## Acceptance Criteria

- [x] NEW: `crates/quota-router-core/src/guardrails/engine.rs`
- [x] NEW: `crates/quota-router-core/src/guardrails/adapter.rs`
- [x] 4 Prometheus metrics defined + registered
- [x] LiteLLM `input_guardrails`/`output_guardrails` aliases
- [x] Per-key `key_overrides` populated from config
- [x] Per-model `model_overrides` populated from config
- [x] Structured logging (RFC-0905) on errors
- [x] `cargo test -p quota-router-core --lib` green (1555/1555)
- [x] `cargo clippy -p quota-router-core --all-targets -- -D warnings` clean
- [x] `cargo fmt --all` clean
- [x] ENGINE-LEVEL: 10 new unit tests covering allow/block/latency/
  executor_from_config/serde-alias round-trip
- [x] ADAPTER-LEVEL: 6 new unit tests covering PII detection + regex
  filter construction + topic restriction + prompt injection threshold
- [x] Memory card filed

## Implementation Notes

- `executor_from_config` returns `None` when ALL of (input, output,
  model_overrides, key_overrides) are empty. This lets operators disable
  the engine cleanly via empty config without leaving the executor
  initialized to no-op.
- `GuardrailResult::Log` is mapped to `GuardrailResult::Warn { warnings }`
  at the adapter boundary because the executor's `Block > Transform > Warn > Allow`
  ordering collapses Log into the same precedence slot. The `Log` action
  preserves *intent* (no warning surfaced) but the executor's resolution
  cannot represent that distinction — this is documented as a substrate
  limitation, not a fix.
- `GuardrailExecutor::check_input` already implements global → model → key
  precedence with short-circuit on Block. The engine just exposes it.
- 4 new metrics live alongside existing `callback_dropped_total`; both
  RFC-0946 and RFC-0947 use the same `Metrics` struct.

## Deferred work (explicit, not unspecified)

- **`proxy.rs` server-fn wrapping** — separate mission. The wrapper sits
  ABOVE `handle_request` and is a single `service_fn` closure that calls
  `engine.check_input(&body, key_id, model)` before dispatch and
  `engine.check_output(&response, key_id, model)` after. Body
  parsing already exists in `handle_request` line ~1500+; the wrapping
  closure can capture the engine by `Arc::clone` without touching the
  11k-LoC handler.

- **HTTP response shaping on Block** — separate mission. The engine
  returns `GuardrailExecutionResult`; the proxy layer turns `Block { .. }`
  into a 400 response with the reason in the body. Pattern matches
  existing `FallbackExecutor` Block handling.

## Cross-references

- RFC-0946 (Economics): Guardrails Framework
- Mission `0946-a` (commit `e46949d0`) — `Guardrail` enum + `GuardrailChecker`
- Mission `0946-b` (commit `09766bc0`) — 6 built-in guardrail types
- RFC-0937 — Prometheus metrics schema
- RFC-0905 — Structured logging
- `crates/quota-router-core/src/guardrails/mod.rs` — substrate
- `crates/quota-router-core/src/metrics.rs` — metrics integration

## Version History

| Version | Date       | Status   | Changes |
| ------- | ---------- | -------- | ------- |
| v0.1    | 2026-08-11 | claimed  | Mission moved `open/` → `claimed/`; engine + adapter + metrics scaffold |
| v0.2    | 2026-08-11 | closed   | Engine + adapter + 4 metrics + serde aliases LANDED; 10+6 new tests; proxy.rs wiring deferred honestly |
