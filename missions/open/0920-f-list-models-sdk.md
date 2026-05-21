# Mission: list_models Python SDK Function

## Status

Open

## RFC

RFC-0920: Unified Python SDK — Dual-Mode LiteLLM/any-llm Compatibility

## Summary

Implement `list_models()` and `alist_models()` with required `provider` parameter.

## Current State

Stub implementation that raises `NotImplementedError`. The `pricing` module has model listing capability that needs wiring to PyO3.

## Acceptance Criteria

- [ ] `list_models(provider="openai")` returns `Sequence[Model]`
- [ ] `list_models()` without provider raises TypeError
- [ ] `list_models(provider, api_key="...", api_base="...")` works with overrides
- [ ] `list_models(provider, client_args={...})` works
- [ ] `alist_models()` returns coroutine, same result as sync
- [ ] `Model` objects have `.id`, `.name`, `.provider`, `.created` fields
- [ ] Error handling: no key → `AuthenticationError`, bad provider → `UnsupportedProviderError`

## Key Files

| File | Change |
|------|--------|
| `crates/quota-router-pyo3/src/completion.rs` | Wire list_models to pricing/provider registry |
| `crates/quota-router-core/src/pricing.rs` | Has model listing capability |

## Claimant

Unclaimed

## Pull Request

None

## Dependencies

- RFC-0920 list_models signature spec
- Provider registry for model enumeration
