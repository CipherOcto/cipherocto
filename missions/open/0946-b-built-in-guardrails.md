# Mission: 0946-b — Built-in Guardrails

## Status

Open

## RFC

RFC-0946 (Economics): Guardrails Framework

## Dependencies

- Mission-0946-a: Guardrail Trait and Registry

## Acceptance Criteria

- [ ] Implement `PiiDetector` — regex-based detection of emails, SSNs, credit cards, phone numbers
- [ ] `PiiMatch` uses `redacted_value` (never raw PII in logs)
- [ ] `PiiDetector.detect()` returns `Vec<PiiMatch>` with start/end positions
- [ ] Implement `PromptInjectionDetector` — pattern matching for injection attempts
- [ ] `PromptInjectionDetector.detect()` returns `Result<f64, GuardrailError>`
- [ ] Implement `ContentModeration` — calls OpenAI-compatible moderation API
- [ ] `ContentModeration` has configurable `timeout_ms` (default 2000), `retries` (default 1), `fallback`
- [ ] Implement `TopicRestriction` — keyword-based matching with stemming
- [ ] Implement `RegexFilter` — user-defined regex patterns with inline flag syntax `(?i)`, `(?m)`, `(?s)`
- [ ] Implement `Custom` guardrail — Python SDK only, configurable `timeout_ms` (default 100ms), `memory_limit_bytes` (default 10MB)
- [ ] Clippy passes with zero warnings
- [ ] All existing tests pass

## Claimant

(unclaimed)

## Pull Request

#

## Notes

Key files:
- `crates/quota-router-core/src/guardrails/pii.rs` — New
- `crates/quota-router-core/src/guardrails/injection.rs` — New
- `crates/quota-router-core/src/guardrails/moderation.rs` — New
- `crates/quota-router-core/src/guardrails/topic.rs` — New
- `crates/quota-router-core/src/guardrails/regex.rs` — New
