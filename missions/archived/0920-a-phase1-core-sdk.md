# Mission: RFC-0920 Phase 1 — Core SDK Foundation

## Status

COMPLETE — all acceptance criteria met (2026-05-12)

## RFC

RFC-0920: Unified Python SDK — Dual-Mode LiteLLM/any-llm Compatibility

## RFC-0920 Role: API Surface + Type Marshaling (Binding Layer)

**RFC-0920 is ONLY for API surface and type marshaling (binding layer).**

**RFC-0917 is the definitive source for ALL heavy lifting:**
- Routing strategies (8 strategies)
- Provider dispatch logic (reqwest for litellm-mode, PyO3 delegation for any-llm-mode)
- State management (ProviderWithState, RouterState)
- Request/response processing
- Budget and rate limiting
- Cache management
- `native_http` module (reqwest providers for liteLLM-mode)

**PyO3 bindings (quota-router-pyo3) should be a thin marshal layer — NO heavy lifting.**

## Dependencies

None — this is the foundational phase.

---

## Architecture Note (RFC-0917)

**RFC-0917 says:**
- **any-llm-mode:** PyO3 → official Python SDKs (PyO3 is thin binding, heavy lifting in core)
- **liteLLM-mode:** Rust `reqwest` → provider REST APIs (NOT implemented)

**Current state:** Phase 1 replaces mock implementations with real provider SDK calls via PyO3 for any-llm-mode.

---

## Phase 1 Checklist (per RFC-0920 §Phase 1 Parameters)

- [x] **Replace mock completion()** — real OpenAI SDK via PyO3 (`AsyncOpenAI` client)
- [x] **Replace mock acompletion()** — async wrapper (Phase 3 for real async)
- [x] **text_completion() / atext_completion()** — per RFC-0920 §LiteLLM Compatibility (LiteLLM parity)
- [x] **Provider resolution algorithm** — both styles work (provider= and provider:model)
- [x] **Exception hierarchy with error codes** — 18 exceptions with proper `__init__` per RFC-0920 spec (§Exception Hierarchy)
- [x] **Basic test suite** — OpenAI + Anthropic (per RFC-0920 §Phase 1 Acceptance Criteria)

---

## Drop-in Replacement Checklist

All exception `__init__` signatures MUST match any-llm for drop-in replacement.

| Exception | Spec (RFC-0920) | Status |
|-----------|-----------------|--------|
| `QuotaRouterError` | `(message, code, status=0, provider=None, details={})` | ✅ |
| `AuthenticationError` | `(message, code="auth_error", **kwargs)` | ✅ |
| `RateLimitError` | `(message, code="rate_limit_exceeded", retry_after=None, **kwargs)` | ✅ |
| `InvalidRequestError` | `(message, code="invalid_request", param=None, **kwargs)` | ✅ |
| `ProviderError` | `(message, code="provider_error", upstream_code=None, **kwargs)` | ✅ |
| `ContentFilterError` | `(message, code="content_filter", **kwargs)` | ✅ |
| `ModelNotFoundError` | `(message, code="model_not_found", **kwargs)` | ✅ |
| `ContextLengthExceededError` | `(message, code="context_length_exceeded", max_tokens=None, received_tokens=None, model=None, **kwargs)` | ✅ |
| `MissingApiKeyError` | `(message, code="missing_api_key", provider="", env_var_name="", **kwargs)` | ✅ |
| `UnsupportedProviderError` | `(message, code="unsupported_provider", provider_key="", supported_providers=[], **kwargs)` | ✅ |
| `UnsupportedParameterError` | `(message, code="unsupported_parameter", param="", provider="", **kwargs)` | ✅ |
| `InsufficientFundsError` | `(message, code="insufficient_funds", current_balance: i64 μunits, required: i64 μunits, **kwargs)` | ✅ |
| `UpstreamProviderError` | `(message, code="upstream_provider_error", status_code=None, **kwargs)` | ✅ |
| `GatewayTimeoutError` | `(message, code="gateway_timeout", **kwargs)` | ✅ |
| `BatchNotCompleteError` | `(message, code="batch_not_complete", batch_id="", status="", status_code=None, **kwargs)` | ✅ |
| `AllModelsFailedError` | `(message, code="all_models_failed", models=[], **kwargs)` | ✅ |
| `BatchPartialFailureError` | `(message, code="batch_partial_failure", successful=[], failed=[], **kwargs)` | ✅ |
| `LengthFinishReasonError` | `(message, code="length_finish_reason", finish_reason="", model=None, **kwargs)` | ✅ |
| `ContentFilterFinishReasonError` | `(message, code="content_filter_finish_reason", finish_reason="", **kwargs)` | ✅ |

**Drop-in alias:** `AnyLLMError = QuotaRouterError` ✅

---

## Acceptance Criteria

- [x] `from quota_router import AnyLLMError` works — legacy code catches this unchanged ✅
- [x] `from quota_router import QuotaRouterError` works — RFC-0920 internal naming ✅
- [x] Exception constructors match any-llm signatures (drop-in compat)
- [x] Real OpenAI SDK call replaces mock echo
- [x] Real Anthropic SDK call replaces mock echo
- [x] `text_completion()` / `atext_completion()` — working per RFC-0920 §LiteLLM Compatibility
- [x] `cargo clippy -D warnings` passes
- [x] `cargo test --lib` passes

---

## Architecture Summary

**RFC-0920 scope (binding layer):**
- API surface definitions (function signatures, exceptions)
- Type marshaling (Python ↔ Rust conversions)
- Provider resolution (parsing provider:model strings)

**NOT RFC-0920 scope (RFC-0917 heavy lifting):**
| Component | RFC-0917 Requirement | Current Implementation |
|-----------|---------------------|----------------------|
| Provider integration (any-llm) | PyO3 → Python SDKs (thin binding) | ✅ OpenAI, Anthropic, Mistral, Gemini, etc. |
| Provider integration (liteLLM) | Rust reqwest → REST APIs | ❌ NOT implemented |
| PyO3 Router class | Thin wrapper to Rust core RouterHandle | ❌ Local routing in Python (WRONG) |
| Routing strategies | 8 strategies in Rust core | ✅ (UsageBasedV2 just added) |
| State management | Rust core (RouterState, ProviderWithState) | ✅ |

**⚠️ ARCHITECTURE ISSUE:** PyO3 Router has local routing (heavy lifting in wrong place).

Phase 1 is complete for any-llm-mode Python SDK foundation.

**Phase 2-4 items are binding layer only — heavy lifting stays in RFC-0917/Rust core.**