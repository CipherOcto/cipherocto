# Mission: 0946-a — Guardrail Trait and Registry

## Status

Open

## RFC

RFC-0946 (Economics): Guardrails Framework

## Dependencies

- RFC-0936: Pre-call Checks (TokenLimit delegates to ContextWindowCheck)

## Acceptance Criteria

- [ ] Define `GuardrailType` enum (ContentFilter, PiiDetection, JailbreakDetection, TopicRestriction, OpenAiModeration, CustomCode, ExternalApi, TokenLimit)
- [ ] Define `GuardrailAction` enum (Block, Warn, Log, Transform)
- [ ] Define `GuardrailResult` enum (Allow, Block, Warn, Transform, Error)
- [ ] Define `GuardrailFallback` enum (FailOpen, FailClosed)
- [ ] Define `Guardrail` trait with `check_input()` and `check_output()` methods
- [ ] Implement `GuardrailRegistry` with `HashMap<String, Box<dyn Guardrail>>`
- [ ] Implement `init_guardrails()` function to register all built-in guardrails
- [ ] Implement `GuardrailExecutor` with global/model/key override merging
- [ ] Execution order: global → model → key, short-circuit on Block
- [ ] Add `GuardrailConfig` to `config.rs`
- [ ] Error variant: `GuardrailResult::Error` consumed by executor (FailOpen → Allow with warning, FailClosed → Block)
- [ ] Clippy passes with zero warnings
- [ ] All existing tests pass

## Claimant

(unclaimed)

## Pull Request

#

## Notes

Key files:
- `crates/quota-router-core/src/guardrails/mod.rs` — New
- `crates/quota-router-core/src/guardrails/registry.rs` — New
- `crates/quota-router-core/src/config.rs` — Add GuardrailConfig
