# Mission: RFC-0920 Phase 1 — Core SDK Foundation

## Status

Open — RFC-0920 Accepted v1.58

## RFC

RFC-0920 (Economics): Unified Python SDK — Dual-Mode LiteLLM/any-llm Compatibility (Accepted v1.58)

## Dependencies

None — this is the foundational phase.

## Context

Phase 1 replaces current mock implementations with real provider SDK calls. OpenAI and Anthropic are the priority providers because:
1. OpenAI covers `provider=model` style (LiteLLM compatibility)
2. Anthropic covers `provider:model` style (any-llm compatibility)

**Current state:** `quota-router-pyo3` completion functions are mock stubs that echo messages. Phase 1 replaces these with real provider SDK calls.

## Phase 1 Checklist (RFC-0920 lines 4597-4611)

- [ ] **Replace mock completion()** — real OpenAI SDK via PyO3 (`AsyncOpenAI` client)
- [ ] **Replace mock acompletion()** — real async OpenAI SDK
- [ ] **text_completion() / atext_completion()** — per RFC-0920 lines 3830-3858 (LiteLLM parity) **A2 fix: corrected line refs**
- [ ] **Provider resolution algorithm** — both styles work (provider= and provider:model)
- [ ] **Exception hierarchy with error codes** — 19 exceptions with proper `__init__` per RFC-0920 spec (lines 623-791) **G1/G4 fix: updated from "16" to "19"**
- [ ] **Async iterator bridge** — `async_iter_to_sync_iter()` for sync streaming
- [ ] **Basic test suite** — OpenAI + Anthropic (per RFC-0920 line 4603) **A5 fix: added**

**A1 fix:** `messages() / amessages()` is Phase 2 (per RFC-0920 line 4615: "Anthropic provider integration"). Phase 1 replaces mock completion() only.

## Drop-in Replacement Checklist (Critical)

This section is the PRIMARY goal — NOT per-RFC internal naming.

### Exception `__init__` Signature Alignment (RFC-0920 lines 623-791)

All exception `__init__` signatures MUST match any-llm for drop-in replacement. **G1/G4 fix:** Table expanded from 10 to 19 rows to cover ALL exceptions.

| Exception | Spec (RFC-0920) | Current | Gap |
|-----------|-----------------|---------|-----|
| `QuotaRouterError` | `(message, code, status=0, provider=None, details={})` | ✅ | None |
| `AuthenticationError` | `(message, code="auth_error", **kwargs)` | ❌ not impl | Missing |
| `RateLimitError` | `(message, code="rate_limit_exceeded", retry_after=None, **kwargs)` | ❌ missing `retry_after`, `code` | Missing |
| `InvalidRequestError` | `(message, code="invalid_request", param=None, **kwargs)` | ❌ not impl | Missing |
| `ProviderError` | `(message, code="provider_error", upstream_code=None, **kwargs)` | ❌ not impl | Missing |
| `ContentFilterError` | `(message, code="content_filter", **kwargs)` | ❌ not impl | Missing |
| `ModelNotFoundError` | `(message, code="model_not_found", **kwargs)` | ❌ not impl | Missing |
| `ContextLengthExceededError` | `(message, code="context_length_exceeded", max_tokens=None, received_tokens=None, **kwargs)` | ❌ not impl | Missing |
| `MissingApiKeyError` | `(message, code="missing_api_key", provider="", env_var_name="", **kwargs)` | ❌ only `(message, provider)` | Missing `code` defaults, `env_var_name` |
| `UnsupportedProviderError` | `(message, code="unsupported_provider", provider_key="", supported_providers=[], **kwargs)` | ❌ only `(message, provider)` | Missing `code` defaults, `provider_key`, `supported_providers` |
| `UnsupportedParameterError` | `(message, code="unsupported_parameter", param="", provider="", **kwargs)` | ❌ only `(message, parameter)` | Wrong field name, missing `code` defaults |
| `InsufficientFundsError` | `(message, code="insufficient_funds", current_balance: int μunits, required: int μunits, **kwargs)` | ❌ uses `f64` | Wrong type, missing `code` defaults |
| `UpstreamProviderError` | `(message, code="upstream_provider_error", status_code=None, **kwargs)` | ❌ not impl | Missing `status_code`, `code` defaults |
| `GatewayTimeoutError` | `(message, code="gateway_timeout", **kwargs)` | ❌ not impl | Missing |
| `BatchNotCompleteError` | `(message, code="batch_not_complete", batch_id="", status="", **kwargs)` | ❌ not impl | **G3 fix:** spec is correct — impl is missing |
| `AllModelsFailedError` | `(message, code="all_models_failed", models=[], **kwargs)` | ❌ not impl | Missing |
| `BatchPartialFailureError` | `(message, code="batch_partial_failure", successful=[], failed=[], **kwargs)` | ❌ not impl | Missing |
| `LengthFinishReasonError` | `(message, code="length_finish_reason", finish_reason="", **kwargs)` | ✅ | None |
| `ContentFilterFinishReasonError` | `(message, code="content_filter_finish_reason", finish_reason="", **kwargs)` | ✅ | None |

**G2 fix:** All signatures include `code=` default values per RFC-0920.

**Drop-in alias (DONE):** `AnyLLMError = QuotaRouterError` ✅

### Return Type Alignment

- [ ] `completion()` — must return dict matching OpenAI `ChatCompletion` structure (not mock echo)
  - **C4 gap:** RFC-0920 spec does NOT define exact dict shape — this is a spec gap, not implementation
- [ ] `text_completion()` / `atext_completion()` — per RFC-0920 lines 3830-3858 (LiteLLM parity)
- [ ] Streaming — **SSE normalization is Phase 3** per RFC-0920 lines 2086, 2088-2154; Phase 1 only has async bridge

## Acceptance Criteria

- [ ] `from quota_router import AnyLLMError` works — legacy code catches this unchanged ✅ (alias added)
- [ ] `from quota_router import QuotaRouterError` works — RFC-0920 internal naming ✅
- [ ] Exception constructors match any-llm signatures (drop-in compat)
- [ ] Real OpenAI SDK call replaces mock echo
- [ ] Real Anthropic SDK call replaces mock echo
- [ ] `text_completion()` / `atext_completion()` — working per RFC-0920 lines 3830-3858
- [ ] `cargo clippy -D warnings` passes
- [ ] `cargo test --lib` passes

## Notes

**C4 Spec Gap:** RFC-0920 does not define the exact `ChatCompletion` dict structure. The spec says "dict matching OpenAI ChatCompletion structure" but provides no concrete schema. This is a spec gap — implementation cannot be verified until RFC-0920 is updated with the actual dict shape.

**Rust-owns-all-heavy-lifting:** Python SDK is a thin PyO3 binding layer. All routing, caching, concurrency, telemetry, rate limiting, spend tracking happens in `quota-router-core` (Rust). Python only provides API surface, type marshaling, and exception translation.