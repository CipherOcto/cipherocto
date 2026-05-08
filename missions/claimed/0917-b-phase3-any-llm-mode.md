# Mission: RFC-0917 Phase 3 — any-llm Mode Implementation

## Status

In Progress — Spec-vs-implementation gap analysis complete (2026-05-07)

**Completed:** API surface (20 functions, 16 exceptions, model parsing, streaming structure, set_api_key, get_budget_status, get_metrics, Python SDK package)

**Remaining:** 41 provider integrations + spec-implementation alignment fixes

**Critical gaps identified:**
- Base class naming: spec says `QuotaRouterError`, impl uses `AnyLLMError` — drop-in replacement will break ✅ FIXED
- Streaming: mock word-split, not actual SSE parsing (`_AnthropicSSEParser`, `parse_openai_sse` spec-only)
- Metrics: in-memory counters, not Prometheus dict per spec
- All functions are mocks/stubs — no actual provider SDK calls

## Architecture Note (RFC-0917)

PyO3 crate (`quota-router-pyo3`) is a **thin Python binding layer**, NOT a full implementation:
- It wraps `quota-router-core` via `KeyStorage` trait, not in-memory stubs
- Provider implementations (openai.rs, anthropic.rs, etc.) have `LLMProvider` trait + mock completion/embedding
- The `ensure_client()` pattern in providers shows PyO3 → Python SDK bridge scaffolding exists but is unused
- Actual provider SDK calls (PyO3 → official Python SDKs) are the remaining Phase 3 work

The gap between spec and implementation is primarily about **real SDK integration**, not missing infrastructure.

## RFC

RFC-0917: Dual-Mode Query Router (Accepted v2.36)

## Dependencies

- Mission: RFC-0917 Alignment ✅ COMPLETED — LatencyTracker + QuotaRouterError + feature gates

## Context

**any-llm-mode replaces any-llm completely.** It is a drop-in replacement for the any-llm Python SDK at `/home/mmacedoeu/_w/ai/any-llm/src/`. The goal is to reimplement the same API surface, same 41 providers, same 20 API functions, but in Rust with PyO3 bindings to quota-router-core.

any-llm is NOT a dependency to delegate to — it is the **spec source** for what the Rust/PyO3 implementation must provide.

## Phase 3 Scope (from any-llm SDK audit)

### Providers — 41 total (must all be supported in any-llm-mode)
```
anthropic, azure, azureanthropic, azureopenai, bedrock, cerebras, cohere,
dashscope, databricks, deepseek, fireworks, gateway, gemini, groq, huggingface,
inception, llama, llamacpp, llamafile, lmstudio, minimax, mistral, moonshot,
mzai, nebius, ollama, openai, openrouter, perplexity, platform, portkey,
sagemaker, sambanova, together, vertexai, vertexaianthropic, vllm, voyage,
watsonx, xai, zai
```

### API Functions — 20 (all must be callable via PyO3)
```
completion(), acompletion()
responses(), aresponses()
messages(), amessages()
embedding(), aembedding()
list_models(), alist_models()
create_batch(), acreate_batch()
retrieve_batch(), aretrieve_batch()
cancel_batch(), acancel_batch()
list_batches(), alist_batches()
retrieve_batch_results(), aretrieve_batch_results()
```

### Exceptions — any-llm exception hierarchy
```
AnyLLMError (base)
├── RateLimitError
├── AuthenticationError
├── InvalidRequestError
├── ProviderError
├── ContentFilterError
├── ModelNotFoundError
├── ContextLengthExceededError
├── MissingApiKeyError
├── UnsupportedProviderError
├── UnsupportedParameterError
├── InsufficientFundsError
├── UpstreamProviderError
├── GatewayTimeoutError
├── LengthFinishReasonError
├── ContentFilterFinishReasonError
└── BatchNotCompleteError
```

### Additional functions needed (from any-llm internal audit)
- `set_api_key()` — validates and registers key with storage
- `get_budget_status()` — returns current spend vs limit
- `get_metrics()` — returns Prometheus metrics dict
- Streaming support (async generators)
- Model string parsing (both `provider/model` and `provider:model` formats)

## Phase 3 Checklist (RFC-0917 lines 1571-1583) — ACTUAL STATUS

- [ ] **PyO3 bridge** — quota-router-pyo3 calls official Python SDKs via PyO3
- [ ] **41 Provider integrations** via PyO3 calls to: `anthropic`, `openai`, `mistralai`, `ollama`, `google-genai` + 36 more
- [x] **Python SDK package** (pyproject.toml + python/quota_router/__init__.py)
- [x] **20 API functions** via PyO3 — MOCKS ONLY, no real provider calls
- [ ] **Streaming via PyO3 async generators** — SPEC: `_AnthropicSSEParser`, `parse_openai_sse`; IMPL: mock word-split chunks
- [x] **Exception hierarchy** — FIXED: base class renamed from `AnyLLMError` to `QuotaRouterError` (2026-05-07)
- [x] `set_api_key()` / `get_budget_status()` — Thin wrapper layer; in-memory for now per RFC-0917 architecture
- [ ] `get_metrics()` — IMPL: in-memory counter; not Prometheus dict per spec
- [x] **Model string parsing** (`provider/model` and `provider:model` formats) — works correctly

### Spec-vs-Implementation Alignment (2026-05-07)

**Fixed:** `QuotaRouterError` base class renamed from `AnyLLMError` (2026-05-07)

**Remaining gaps (Phase 3 real SDK integration):**

| Item | Spec (RFC-0917) | Implementation | Gap |
|------|-----------------|----------------|-----|
| Streaming | `_AnthropicSSEParser`, `parse_openai_sse` (stateful SSE) | Mock word-split chunks | Not implemented |
| `get_metrics` | Prometheus dict | In-memory counter | Not Prometheus |
| Provider SDK calls | PyO3 → official Python SDKs | `LLMProvider` trait with mock impls | Scaffolding exists, calls not wired |
| `execute_with_retry` + `ProviderRequest` trait | Spec §Fallback | Not in `fallback.rs` | Spec-only pattern |
| `parse_response` as associated function | Spec §Response Parsing | Not in impl | Spec-only pattern |
| `into_future` (PyO3) | Spec §GIL Table | Not in impl | Spec-only pattern |
| `_get_server_secret` with RuntimeError | Spec §Server Secret | Not in impl | Spec-only pattern |

**PyO3 architecture per RFC-0917:** The quota-router-pyo3 crate is a thin wrapper layer. Provider implementations use `LLMProvider` trait with `ensure_client()` pattern showing PyO3 → Python SDK bridge scaffolding. Actual provider integration (41 providers) remains the core Phase 3 work beyond API surface.

## Acceptance Criteria

- [x] quota-router-pyo3 exposes all 20 API functions via PyO3 — MOCKS ONLY
- [ ] All 41 providers accessible via any-llm-mode (requires PyO3 integration with Python SDKs)
- [x] Exception hierarchy matches spec — FIXED: base class renamed from AnyLLMError to QuotaRouterError
- [x] `set_api_key()` / `get_budget_status()` — Thin wrapper layer per RFC-0917 architecture note
- [ ] `get_metrics()` — returns in-memory counter; not Prometheus dict per spec
- [x] Model string parsing handles `provider/model` and `provider:model` (parse_model, parse_model_strict) — works correctly
- [ ] Streaming uses actual SSE parsing (`_AnthropicSSEParser`, `parse_openai_sse`) — CURRENTLY mock word-split
- [x] `cargo clippy -D warnings` passes ✅ (2026-05-07)
- [x] `cargo test --lib` passes ✅ (14 tests, 2026-05-07)
