# Mission: Embedding Python SDK Functions

## Status

Open

## RFC

RFC-0920: Unified Python SDK — Dual-Mode LiteLLM/any-llm Compatibility

## Summary

Implement `embedding()` and `aembedding()` with dual-convention parameter support (`input` for litellm, `inputs` for any-llm).

## Current State

Stub implementations that raise `NotImplementedError`. The `native_http` providers already have `HttpProvider::embedding()` trait implementations (OpenAI, Perplexity, Bedrock). The PyO3 layer needs wiring.

## Acceptance Criteria

- [ ] `embedding(model, input=["hello"])` works (litellm convention)
- [ ] `embedding(model, inputs=["hello"])` works (any-llm convention)
- [ ] `embedding(model, input=["a"], inputs=["b"])` raises error (ambiguous)
- [ ] `embedding(model)` raises error (no input)
- [ ] `embedding()` returns `EmbeddingResponse` with `.data`, `.model`, `.usage`
- [ ] `aembedding()` returns coroutine, same result as sync
- [ ] `client_args` parameter accepted
- [ ] All drop-in litellm embedding tests pass
- [ ] All drop-in any-llm embedding tests pass
- [ ] Error handling: invalid model → `ModelNotFoundError`, no key → `AuthenticationError`

## Key Files

| File | Change |
|------|--------|
| `crates/quota-router-pyo3/src/completion.rs` | Wire embedding to native_http provider |
| `crates/quota-router-core/src/native_http/mod.rs` | Already has `HttpProvider::embedding()` |

## Claimant

Unclaimed

## Pull Request

None

## Dependencies

- RFC-0920 embedding signature spec (dual-convention)
- native_http embedding implementations (already exist)
