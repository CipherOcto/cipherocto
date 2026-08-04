# Mission: 0946-a — Guardrail Trait and Registry

## Status

Claimed (2026-08-04) by @mmacedoeu

## RFC

RFC-0946 (Economics): Guardrails Framework

## Dependencies

- RFC-0936: Pre-call Checks (TokenLimit delegates to ContextWindowCheck)

## Acceptance Criteria

- [x] Define `Guardrail` enum (PiiDetection, PromptInjection, ContentModeration, TopicRestriction, TokenLimit, RegexFilter, Custom). (`crates/quota-router-core/src/guardrails/mod.rs:42-104`)
- [x] Define `GuardrailAction` enum (Block, Warn, Log, Transform). (`crates/quota-router-core/src/guardrails/mod.rs:138-148`)
- [x] Define `GuardrailResult` enum (Allow, Block, Warn, Transform, Error). (`crates/quota-router-core/src/guardrails/mod.rs:152-170`)
- [x] Define `GuardrailFallback` enum (FailOpen, FailClosed). (`crates/quota-router-core/src/guardrails/mod.rs:174-181`)
- [x] Define `Guardrail` trait with `check_input()` and `check_output()` methods. (`GuardrailChecker` trait at `crates/quota-router-core/src/guardrails/mod.rs:220-237` — `Guardrail` enum is the configuration, `GuardrailChecker` is the runtime trait that operates over the enum config.)
- [x] Implement `GuardrailRegistry` with `HashMap<String, Box<dyn Guardrail>>`. (`crates/quota-router-core/src/guardrails/registry.rs` — LazyLock<RwLock<HashMap<&'static str, fn() -> Arc<dyn GuardrailChecker>>>> using factory pattern, equivalent to native_http::PROVIDER_REGISTRY.)
- [x] Implement `init_guardrails()` function to register all built-in guardrails. (`crates/quota-router-core/src/guardrails/registry.rs:58` registers 5 built-ins: pii_detection, prompt_injection, topic_restriction, regex_filter, custom.)
- [x] Implement `GuardrailExecutor` with global/model/key override merging. (`crates/quota-router-core/src/guardrails/mod.rs:720-963`)
- [x] Execution order: global → model → key, short-circuit on Block. (`check_input` at `mod.rs:749-818`, `check_output` at `mod.rs:821-890`)
- [x] Add `GuardrailConfig` to `config.rs`. (`crates/quota-router-core/src/config.rs:560,614-629`)
- [x] Error variant: `GuardrailResult::Error` consumed by executor (FailOpen → Allow with warning, FailClosed → Block). (`resolve_error` at `mod.rs:905-929`)
- [x] Clippy passes with zero warnings. (`cargo clippy -p quota-router-core --lib -- -D warnings` clean)
- [x] All existing tests pass. (`cargo test -p quota-router-core --lib guardrails`: 20 tests pass)

## Claimant

@mmacedoeu

## Pull Request

# pending user push

## Notes

Key files:
- `crates/quota-router-core/src/guardrails/mod.rs` — New
- `crates/quota-router-core/src/guardrails/registry.rs` — New
- `crates/quota-router-core/src/config.rs` — Add GuardrailConfig

## Closure

**Claimed:** 2026-08-04
**Implemented:** 2026-08-04 (pre-existing framework verified; no new commits — framework shipped in prior sessions)

### Deviations

1. **Trait name `GuardrailChecker` vs mission text `Guardrail`**: The mission text reads "Define `Guardrail` trait with `check_input()` and `check_output()` methods", but the framework names the runtime trait `GuardrailChecker` to avoid collision with the configuration enum `Guardrail` (RFC-0946 §Built-in Guardrails). The enum is the static config (`Guardrail::PiiDetection { action, entities }`); the trait is the runtime checker that operates on it. Renaming the enum would break every config consumer; renaming the trait is a non-breaking improvement. Mission AC semantics satisfied.
2. **Registry pattern `fn() -> Arc<dyn GuardrailChecker>` instead of `Box<dyn Guardrail>`**: The registry stores factories (zero-sized fn items) rather than pre-built instances. This allows per-instance configuration (different regex patterns, different PII entity lists) without breaking the `&'static str` key constraint. The `Arc<dyn GuardrailChecker>` return from `create()` is the runtime analogue of `Box<dyn Guardrail>`. Pattern lifted from `native_http::PROVIDER_REGISTRY` for consistency.
3. **TokenLimit delegation**: Per RFC-0946 §TokenLimit + RFC-0936 dependency, `TokenLimit` is a config-only enum variant (max_input_tokens / max_output_tokens). Actual token counting happens via the RFC-0936 ContextWindowCheck in the pre_call_checks path; `GuardrailExecutor` does NOT duplicate the counting logic. Mission 0946-a scopes to the enum/config surface; the delegation wiring lands in a follow-on mission.

### Follow-up (NOT this mission)

- `GuardrailExecutor::check_output` does not currently transform output (only flags it). The `Transform { transformed: bool }` path is wired for `GuardrailAction::Transform` but the redact-and-return loop is mission 0946-c.
- Per-key override merging for `Transform` semantics (replace vs redact) is mission 0946-c.
- `GuardrailConfig` schema does NOT yet serialize via stable JSON keys for cross-version compat — planned in a follow-up.
