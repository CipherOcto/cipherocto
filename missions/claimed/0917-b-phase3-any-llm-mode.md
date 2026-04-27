# Mission: RFC-0917 Phase 3 — any-llm Mode Implementation

## Status

In Progress — RFC-0917 Phase 3 vs any-llm comparison complete (2026-04-27)

## RFC

RFC-0917: Dual-Mode Query Router (Accepted v2.24)

## Dependencies

- Mission: RFC-0917 Alignment ✅ COMPLETED (archived) — LatencyTracker + QuotaRouterError

## Summary

Implement RFC-0917 Phase 3 items by building on the any-llm Python SDK. The comparison against `/home/mmacedoeu/_w/ai/any-llm/` reveals significant gaps between the Phase 3 spec and any-llm's current implementation. This mission closes those gaps.

## Phase 3 Checklist (RFC-0917 lines 997-1007)

| # | Item | Status | Notes |
|---|------|--------|-------|
| 1 | PyO3 bridge module calling official Python SDKs | **MISSING** | any-llm uses pure Python, no Rust bindings |
| 2 | Provider SDK integrations: `anthropic`, `openai`, `mistralai`, `ollama`, `google-genai` | **PARTIAL** | Native Python SDKs — not via PyO3 bridge |
| 3 | Python SDK interface (`pip install quota_router`) | **MISSING** | Package is `any-llm-sdk`, not `quota_router` |
| 4 | `completion()` / `acompletion()` / `embedding()` / `aembedding()` | **IMPLEMENTED** | Matches RFC spec |
| 5 | Streaming support (Python generator via PyO3) | **PARTIAL** | Python async generators — not via PyO3 |
| 6 | LiteLLM-compatible exception types | **MISSING** | Different exception hierarchy in any-llm |
| 7 | `set_api_key()` — validates and registers key with storage | **MISSING** | Only `_verify_and_set_api_key()` (env var only) |
| 8 | `get_budget_status()` — returns current spend vs limit | **MISSING** | Function does not exist |
| 9 | `get_metrics()` — returns Prometheus metrics dict | **MISSING** | No public function exposed |
| 10 | Model string parsing (`provider/model` and `provider:model`) | **IMPLEMENTED** | Matches RFC spec |
| 11 | QuotaRouterError unified error type | **SPEC DONE** | Enum spec'd in RFC; From impls + Error traits deferred |

## Full Gaps Analysis

### 1. PyO3 Bridge Module — MISSING
**RFC requirement:** PyO3 bridge calling official Python SDKs from Rust.

any-llm current approach: Pure Python with `importlib` dynamic loading of provider modules. No PyO3, no Rust.

Gap: No Rust→Python bridge exists. The `quota-router-pyo3` crate exists in cipherocto but is not connected to the Python SDK interface.

### 2. Provider SDK Integrations — PARTIAL
**RFC requirement:** `anthropic`, `openai`, `mistralai`, `ollama`, `google-genai`.

any-llm has all five as native Python SDK integrations:
- `src/any_llm/providers/anthropic/anthropic.py`
- `src/any_llm/providers/openai/openai.py`
- `src/any_llm/providers/mistral/mistral.py`
- `src/any_llm/providers/ollama/ollama.py`
- `src/any_llm/providers/gemini/gemini.py`

Gap: These are native Python, not via PyO3 bridge. Per RFC-0917 Phase 3, they should be called via PyO3 from Rust.

### 3. Python SDK Package Name — WRONG NAME
**RFC requirement:** `pip install quota_router`

any-llm: Package is `any-llm-sdk`.

Action needed: Rename package to `quota_router` (or alias `quota-router` with `_` separator).

### 4. API Functions — IMPLEMENTED
`completion()`, `acompletion()`, `embedding()`, `aembedding()` exist in `src/any_llm/api.py`.

### 5. Streaming Support — PARTIAL
**RFC requirement:** Python generator via PyO3.

any-llm: `src/any_llm/gateway/streaming.py` with `async_iter_to_sync_iter`. Native Python async generators, not PyO3.

Gap: Not via PyO3. RFC spec says "Python generator via PyO3."

### 6. LiteLLM-Compatible Exception Types — MISSING
**RFC spec (lines 1206-1268):**
```python
QuotaRouterException(Exception)
├── KeyException
├── BudgetException
├── RouterException
├── RegistryException
├── StorageException
└── ProviderException
EXCEPTION_MAP = { ... }
```

**any-llm actual (`src/any_llm/exceptions.py`):**
```python
AnyLLMError(Exception)
├── RateLimitError, AuthenticationError, InvalidRequestError,
├── ProviderError, ContentFilterError, ModelNotFoundError, ...
```

Gap: Entire exception hierarchy is different. Need to replace with RFC-specified hierarchy.

### 7. `set_api_key()` — MISSING
**RFC requirement:** Validates and registers key with storage.

any-llm: Only `_verify_and_set_api_key()` which reads env vars. No storage registration.

Gap: No function to validate and register keys with storage.

### 8. `get_budget_status()` — MISSING
**RFC requirement:** Returns current spend vs limit.

any-llm: No such function exists anywhere.

Gap: Entire function missing.

### 9. `get_metrics()` — MISSING
**RFC requirement:** Returns Prometheus metrics dict.

any-llm: `src/any_llm/gateway/metrics.py` exists but not exposed as public function.

Gap: No public `get_metrics()` function.

### 10. Model String Parsing — IMPLEMENTED
`AnyLLM.split_model_provider()` handles both `provider/model` and `provider:model` formats. Matches RFC.

### 11. QuotaRouterError — SPEC DONE
RFC spec (lines 1009-1270) fully defines the enum, From impls, Display, Error impl, HTTP mapping, Python class hierarchy. Implementation partially done:
- Enum + StorageError + Clone on KeyError/BudgetError: **DONE**
- From impls (RouterError, RegistryError, StorageError → QuotaRouterError): **MISSING**
- Error trait impls for wrapped types: **MISSING**
- source() chain: **MISSING**

## Implementation Notes

**Package name conflict:** any-llm package name conflicts with RFC-0917's `quota_router` requirement.

**Architecture mismatch:** any-llm is a pure Python SDK with its own exception system, no PyO3 bridge, no budget tracking, no unified error types. RFC-0917 Phase 3 describes a Rust-based quota-router with Python SDK bindings — a fundamentally different architecture.

**Recommended approach:**
1. Keep any-llm's existing Python SDK as-is (it works for pure Python use cases)
2. Create new `quota-router-py` crate that wraps `quota-router-core` via PyO3 with RFC-specified interface
3. Or refactor any-llm to use PyO3 bindings to quota-router-core

This decision affects the entire Phase 3 implementation path.

## Acceptance Criteria

- [ ] `quota_router` Python package callable via `pip install quota-router` (or `quota_router`)
- [ ] `completion()`, `acompletion()`, `embedding()`, `aembedding()` work via PyO3 bridge to quota-router-core
- [ ] `QuotaRouterError` with all From impls and Error trait impls for wrapped types
- [ ] LiteLLM-compatible exception hierarchy (QuotaRouterException + subclasses + EXCEPTION_MAP)
- [ ] `set_api_key()` validates and registers key with storage
- [ ] `get_budget_status()` returns current spend vs limit
- [ ] `get_metrics()` returns Prometheus metrics dict
- [ ] Streaming support via PyO3
- [ ] All 5 provider SDKs accessible via PyO3 bridge
- [ ] `cargo clippy -D warnings` and `cargo test --lib` pass for Rust components
