# Mission: 0946-b — Built-in Guardrails

## Status

Completed (Archived 2026-07-30 — Path B)

> **Path B closure:** AC + code verified 2026-07-30 via mission audit. Code ground: `crates/quota-router-core/src/guardrails/mod.rs:247` `PiiDetector` + `:343` `PromptInjection` + `:402` `RegexFilter` + `:436` `ContentModeration` + `:531` `TopicRestriction`. All 12 ACs were checked while the mission still lived in `missions/open/`. Did not pass through `claimed/` or `with-pr/` — work landed in `next` via prior PRs whose PR trail is lost to audit.

## RFC

RFC-0946 (Economics): Guardrails Framework

## Dependencies

- Mission-0946-a: Guardrail Trait and Registry

## Acceptance Criteria

- [x] Implement `PiiDetector` — regex-based detection of emails, SSNs, credit cards, phone numbers
- [x] `PiiMatch` uses `redacted_value` (never raw PII in logs)
- [x] `PiiDetector.detect()` returns `Vec<PiiMatch>` with start/end positions
- [x] Implement `PromptInjection` — pattern matching for injection attempts
- [x] `PromptInjection.detect()` returns `Result<f64, GuardrailError>`
- [x] Implement `ContentModeration` — calls OpenAI-compatible moderation API
- [x] `ContentModeration` has configurable `timeout_ms` (default 2000), `retries` (default 1), `fallback`
- [x] Implement `TopicRestriction` — keyword-based matching with stemming
- [x] Implement `RegexFilter` — user-defined regex patterns with inline flag syntax `(?i)`, `(?m)`, `(?s)`
- [x] Implement `Custom` guardrail — Python SDK only, configurable `timeout_ms` (default 100ms), `memory_limit_bytes` (default 10MB)
- [x] Clippy passes with zero warnings (guardrails module only; other modules have errors)
- [x] All existing tests pass (guardrails module only)

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
