# Dual-Mode Full Parity — Gap Analysis

**Date:** 2026-05-17
**Goal:** quota-router as drop-in replacement for both LiteLLM and any-llm

---

## Current State

### What's Built

| Category | Count | Details |
|----------|-------|---------|
| HTTP endpoints | 14 | /v1/chat/completions, /v1/completions, /v1/embeddings, /v1/models, /v1/moderations, /v1/messages, /v1/images, /v1/audio/*, /v1/responses, /metrics, /health, /ready, /{provider}/* passthrough |
| native_http providers | 10 | OpenAI, Anthropic, Mistral, Gemini, Azure, Bedrock, Ollama, Groq, Together, Replicate |
| Streaming providers | 7/10 | OpenAI, Anthropic, Mistral, Azure, Ollama, Groq, Together |
| py_bridge providers | 42 | All major providers including HuggingFace, Cohere, DeepSeek, Fireworks, etc. |
| Python SDK functions | 35 | completion, acompletion, embedding, aembedding, messages, responses, batches, router |
| Accepted RFCs | 39 | Full spec coverage for routing, auth, rate limiting, budgets, secrets, metrics |
| Completed missions | 137 | All core infrastructure implemented |
| Tests | 286 | All passing |

### Cross-Cutting Concerns (on /v1/chat/completions)

- Gateway auth (3 header formats, master key bypass, constant-time comparison)
- Per-key RPM rate limiting
- Per-user RPM rate limiting (1000 RPM default)
- Response caching (model + messages hash, TTL-based)
- Fallback chains (exponential backoff on 5xx)
- Rate limit headers (X-RateLimit-Limit, X-RateLimit-Remaining, X-RateLimit-Reset)
- Structured request logging (method, path, model, provider, status, duration, user)
- Balance checking (OCTO-W, 402 on insufficient)

---

## Critical Gaps (P0 — Blocks Drop-in Replacement)

### 1. Python SDK Import Alias

**Gap:** `import quota_router as litellm` and `import quota_router as any_llm` must work identically.

**Status:** RFC-0908 and RFC-0920 accepted. PyO3 bindings exist (`quota-router-pyo3`). But:
- No `__init__.py` with litellm-compatible API surface
- No `quota_router.litellm` or `quota_router.any_llm` alias modules
- Exception classes don't match litellm naming (`AuthenticationError`, `RateLimitError`, etc.)

**RFC needed:** Update RFC-0908 with alias module spec.

### 2. Python SDK Router Class

**Gap:** LiteLLM's `Router(model_list=[...])` class is the primary entry point for production users.

**Status:** RFC-0902 accepted. Router struct exists in Rust. But:
- Python `Router` class not exposed via PyO3
- No `Router.acompletion()`, `Router.aembedding()` methods
- No `Router.get_available_deployment()` method

**RFC needed:** Update RFC-0908 with Router class Python bindings.

### 3. LiteLLM Exception Parity

**Gap:** Python SDK must raise same exception types as litellm.

**Current exceptions in quota-router-pyo3:**
- QuotaRouterError, RateLimitError, AuthenticationError, InvalidRequestError
- ProviderError, ContentFilterError, ModelNotFoundError, ContextLengthExceededError
- MissingApiKeyError, UnsupportedProviderError, UnsupportedParameterError
- InsufficientFundsError, UpstreamProviderError, GatewayTimeoutError

**LiteLLM exceptions not mapped:**
- BudgetExceededError (403) — has InsufficientFundsError but different name
- ServiceUnavailableError (503) — has UpstreamProviderError but different name
- APIConnectionError (502) — missing
- APIError (500) — missing
- NotFoundError (404) — has ModelNotFoundError but different name

**RFC needed:** Update RFC-0920 with exact exception name mapping.

### 4. Config File Compatibility

**Gap:** LiteLLM uses `proxy_config.yaml` with specific structure.

**Status:** RFC-0927 and RFC-0928 accepted. GatewayConfig supports `model_list` alias. But:
- `general_settings` section not mapped (master_key, database_url, etc.)
- `guardrails` section not supported
- `litellm_settings.cache` not wired to response cache
- `litellm_settings.callbacks` not supported

**RFC needed:** RFC-0907 (Configuration Management) — exists as Planned, needs spec.

---

## High Priority Gaps (P1 — Feature Parity)

### 5. Spend Tracking API

**Gap:** LiteLLM has `/spend/logs`, `/global/spend`, `/spend/calculate` endpoints.

**Status:** RFC-0904 accepted. Spend ledger table exists. But:
- No HTTP endpoints for querying spend
- No `/spend/logs` endpoint
- No `/global/spend` aggregate endpoint

**RFC needed:** Update RFC-0904 with API endpoint spec.

### 6. Team Management API

**Gap:** LiteLLM has `/team/new`, `/team/info`, `/team/update`, `/team/list`, `/team/member_add`, `/team/member_delete`.

**Status:** Admin server has basic team CRUD. But:
- Not wired through proxy with auth
- No `/team/list` endpoint
- No member management

**RFC needed:** Update RFC-0932 with team management API spec.

### 7. User Management API

**Gap:** LiteLLM has `/user/new`, `/user/info`, `/user/update`.

**Status:** No user management exists.

**RFC needed:** New RFC for user management.

### 8. Config Hot-Reload

**Gap:** LiteLLM has `/config/update` and `/config/get` endpoints.

**Status:** RFC-0907 exists as Planned but has no spec.

**RFC needed:** RFC-0907 needs full spec.

### 9. Streaming for Remaining Providers

**Gap:** Gemini, Bedrock, Replicate don't support streaming.

**Status:** 7/10 providers support streaming. The remaining 3 need custom SSE parsers.

**RFC needed:** Update RFC-0941 with Gemini/Bedrock/Replicate streaming spec.

### 10. Content Policy Fallback

**Gap:** LiteLLM has `content_policy_fallbacks` in router settings.

**Status:** RFC-0902 spec'd but not implemented.

**RFC needed:** Update RFC-0902 with content policy fallback spec.

---

## Medium Priority Gaps (P2 — Enterprise Features)

### 11. Guardrails

**Gap:** LiteLLM has 20+ guardrail integrations (input/output filtering, PII masking).

**Status:** Not specified.

**RFC needed:** New RFC for guardrails framework.

### 12. Callback System

**Gap:** LiteLLM has `litellm.callbacks = [handler]` for Langfuse, Datadog, custom webhooks.

**Status:** Not specified.

**RFC needed:** New RFC for callback system.

### 13. Prompt Management

**Gap:** LiteLLM has `register_prompt_template()` and `get_prompt_template()`.

**Status:** Not specified.

**RFC needed:** New RFC for prompt management.

### 14. Batch API Processing

**Gap:** LiteLLM has batch completion with model fallback.

**Status:** Python SDK has `batch_completion()` but not exposed via proxy.

**RFC needed:** Update RFC-0908 with batch API spec.

### 15. Enterprise SSO

**Gap:** LiteLLM supports OAuth2, SAML, JWT authentication.

**Status:** Not specified.

**RFC needed:** New RFC for enterprise auth.

---

## RFC Action Items

### New RFCs Required

| RFC | Title | Priority |
|-----|-------|----------|
| RFC-0945 | User Management API | P1 |
| RFC-0946 | Guardrails Framework | P2 |
| RFC-0947 | Callback System | P2 |
| RFC-0948 | Prompt Management | P2 |

### Existing RFCs Requiring Updates

| RFC | Update Required | Priority |
|-----|-----------------|----------|
| RFC-0908 | Add alias modules, Router class bindings, exception mapping | P0 |
| RFC-0920 | Add exact exception name mapping | P0 |
| RFC-0907 | Full config hot-reload spec | P1 |
| RFC-0904 | Add spend tracking API endpoints | P1 |
| RFC-0932 | Add team management API | P1 |
| RFC-0941 | Add Gemini/Bedrock/Replicate streaming | P1 |
| RFC-0902 | Add content policy fallback | P1 |

---

## Mission Action Items

### Phase 1: Critical Path (P0)

| Mission | Description | RFC | Effort |
|---------|-------------|-----|--------|
| 0908-a | Python SDK alias modules | RFC-0908 | 2-3d |
| 0908-b | Python SDK Router class | RFC-0908 | 3-4d |
| 0920-a | Exception name mapping | RFC-0920 | 1-2d |

### Phase 2: API Parity (P1)

| Mission | Description | RFC | Effort |
|---------|-------------|-----|--------|
| 0904-b | Spend tracking API | RFC-0904 | 2-3d |
| 0932-b | Team management API | RFC-0932 | 2-3d |
| 0945-a | User management API | RFC-0945 | 2-3d |
| 0907-a | Config hot-reload | RFC-0907 | 2-3d |
| 0941-b | Gemini/Bedrock/Replicate streaming | RFC-0941 | 3-4d |
| 0902-b | Content policy fallback | RFC-0902 | 2-3d |

### Phase 3: Enterprise (P2)

| Mission | Description | RFC | Effort |
|---------|-------------|-----|--------|
| 0946-a | Guardrails framework | RFC-0946 | 5-7d |
| 0947-a | Callback system | RFC-0947 | 3-4d |
| 0948-a | Prompt management | RFC-0948 | 2-3d |

---

## Summary

| Category | Done | Remaining |
|----------|------|-----------|
| HTTP endpoints | 14 | 6 (spend, team, user, config) |
| native_http streaming | 7/10 | 3 (gemini, bedrock, replicate) |
| Python SDK functions | 35 | 5 (alias, router, batch proxy) |
| Exception types | 13 | 5 (name mapping) |
| RFCs | 39 accepted | 4 new + 7 updates |
| Missions | 137 completed | 12 remaining |

**Estimated remaining effort:** 25-35 days across 12 missions.
