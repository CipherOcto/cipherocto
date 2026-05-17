# Mission: 0948-c — Prompt Integration

## Status

Open

## RFC

RFC-0948 (Economics): Prompt Management

## Dependencies

- Mission-0948-a: Prompt Registry
- Mission-0948-b: Prompt API Endpoints

## Acceptance Criteria

- [ ] Add `prompt_id: Option<String>` and `prompt_variables: Option<HashMap<String, String>>` to `ChatCompletionRequest`
- [ ] Integrate prompt resolution into `proxy.rs` — resolve before provider call
- [ ] `resolve_prompt()` renders template and injects as system message
- [ ] Implement A/B testing — deterministic hashing (API key ID preferred, X-Request-Id fallback, generated UUID)
- [ ] Single source of truth for A/B weights: `AbTest.weight_b` only
- [ ] `AbTestMetrics` uses `AtomicU64` for concurrent counter updates
- [ ] Persist `AbTestMetrics` to stoolap
- [ ] Add Python SDK prompt support
- [ ] Clippy passes with zero warnings
- [ ] All existing tests pass

## Claimant

(unclaimed)

## Pull Request

#

## Notes

Key files:
- `crates/quota-router-core/src/proxy.rs` — Integrate prompt resolution
- `crates/quota-router-core/src/prompts/ab_test.rs` — New
- `crates/quota-router-core/src/python_sdk/mod.rs` — Python prompt support
