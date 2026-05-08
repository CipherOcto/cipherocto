# Mission: RFC-0917 Phase 3 — any-llm Mode Implementation

## Status

In Progress — Real SDK integration underway (2026-05-08)

**Completed:** API surface (20 functions), 18 exceptions, 42 providers, model parsing, SSE streaming parsing (real), set_api_key, get_budget_status, get_metrics (Prometheus format), **OpenAI SDK (real), Anthropic SDK (real), Mistral SDK (real), Gemini SDK (real), Groq SDK (real), Cohere SDK (real), Perplexity SDK (real), DeepSeek SDK (real), Azure SDK (real), Together SDK (real), Fireworks SDK (real), Cerebras SDK (real), OpenRouter SDK (real), xAI SDK (real), HuggingFace SDK (real), Moonshot SDK (real), Voyage SDK (real)**

**Remaining:** 24 provider SDK integrations (ollama, databricks, etc.)

**Critical gaps identified:**
- ✅ FIXED: Base class naming: spec says `QuotaRouterError`, impl uses `AnyLLMError` — FIXED
- ✅ FIXED: Streaming: mock word-split → actual SSE parsing implemented (2026-05-08)
- ✅ FIXED: Metrics: in-memory counters → Prometheus dict format (2026-05-08)
- ✅ FIXED: Provider registry: 6 → 42 providers (2026-05-08)
- ✅ FIXED: OpenAI SDK: mock → real SDK integration via PyO3 (2026-05-08)
- ✅ FIXED: Anthropic SDK: mock → real SDK integration via PyO3 (2026-05-08)
- ✅ FIXED: Mistral SDK: mock → real SDK integration via PyO3 (2026-05-08)
- ✅ FIXED: Gemini SDK: mock → real SDK integration via PyO3 (2026-05-08)
- ✅ FIXED: Groq SDK: mock → real SDK integration via PyO3 (2026-05-08)
- ✅ FIXED: Cohere SDK: mock → real SDK integration via PyO3 (2026-05-08)
- ✅ FIXED: Perplexity SDK: mock → real SDK integration via PyO3 (2026-05-08)
- Remaining: 34 provider SDK integrations (same pattern to replicate)

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

### Exceptions — 18 per RFC-0920 (any-llm hierarchy + quota-router specific)
```
QuotaRouterError (base per RFC-0920)
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
├── BatchNotCompleteError
├── AllModelsFailedError (quota-router specific per RFC-0920)
└── BatchPartialFailureError (quota-router specific per RFC-0920)
```

### Additional functions needed (from any-llm internal audit)
- `set_api_key()` — validates and registers key with storage
- `get_budget_status()` — returns current spend vs limit
- `get_metrics()` — returns Prometheus metrics dict
- Streaming support (async generators)
- Model string parsing (both `provider/model` and `provider:model` formats)

## Phase 3 Checklist (RFC-0917 lines 1571-1583) — ACTUAL STATUS

- [ ] **PyO3 bridge** — quota-router-pyo3 calls official Python SDKs via PyO3
- [ ] **41 Provider integrations** via PyO3 calls — 10 of 41 complete: `openai`, `anthropic`, `mistral`, `gemini`, `groq`, `cohere`, `perplexity`, `deepseek`, `azure`, `together`
- [x] **Python SDK package** (pyproject.toml + python/quota_router/__init__.py)
- [x] **20 API functions** via PyO3 — OpenAI and Anthropic now call real SDKs
- [x] **Streaming via SSE parsing** — `parse_openai_sse`, `parse_anthropic_sse`, `chunks_from_openai_events`, `chunks_from_anthropic_events` ✅ (2026-05-08)
- [x] **Exception hierarchy** — 18 exceptions (incl. AllModelsFailedError, BatchPartialFailureError per RFC-0920)
- [x] `set_api_key()` / `get_budget_status()` — Thin wrapper layer; in-memory for now per RFC-0917 architecture
- [x] `get_metrics()` — Returns Prometheus dict with `quota_router_*` prefixed keys ✅ (2026-05-08)
- [x] **Model string parsing** (`provider/model` and `provider:model` formats) — works correctly
- [x] **Provider registry** — 42 providers wired in Providers::get() ✅ (2026-05-08)
- [x] **OpenAI SDK integration** — real SDK calls via PyO3 ✅ (2026-05-08)
- [x] **Anthropic SDK integration** — real SDK calls via PyO3 ✅ (2026-05-08)

### Spec-vs-Implementation Alignment (2026-05-08)

**Fixed items (P1-P5):**
- ✅ `QuotaRouterError` base class renamed from `AnyLLMError` (P1)
- ✅ Provider registry: 6 → 42 providers (P2)
- ✅ `get_metrics()`: In-memory counter → Prometheus dict with `quota_router_*` prefix (P3)
- ✅ `InsufficientFundsError`: `current_balance`/`required` changed from f64 to i64 (µunits) per RFC-0920 (P4)
- ✅ Streaming: Mock word-split → actual SSE parsing for OpenAI + Anthropic formats (P5)

**Remaining gaps (Phase 3 real SDK integration):**

| Item | Spec (RFC-0917) | Implementation | Gap |
|------|-----------------|----------------|-----|
| Provider SDK calls | PyO3 → official Python SDKs | `LLMProvider` trait with mock impls | Scaffolding exists, calls not wired |
| `execute_with_retry` + `ProviderRequest` trait | Spec §Fallback | Not in `fallback.rs` | Spec-only pattern |
| `parse_response` as associated function | Spec §Response Parsing | Not in impl | Spec-only pattern |
| `into_future` (PyO3) | Spec §GIL Table | Not in impl | Spec-only pattern |
| `_get_server_secret` with RuntimeError | Spec §Server Secret | Not in impl | Spec-only pattern |

**PyO3 architecture per RFC-0917:** The quota-router-pyo3 crate is a thin wrapper layer. Provider implementations use `LLMProvider` trait with `ensure_client()` pattern showing PyO3 → Python SDK bridge scaffolding. Actual provider integration (41 providers) remains the core Phase 3 work beyond API surface.

## Acceptance Criteria

- [x] quota-router-pyo3 exposes all 20 API functions via PyO3
- [x] OpenAI SDK integration — real SDK calls (2026-05-08)
- [x] Anthropic SDK integration — real SDK calls (2026-05-08)
- [ ] 39 remaining provider SDK integrations (same pattern to replicate)
- [x] Exception hierarchy matches spec — 18 exceptions including AllModelsFailedError, BatchPartialFailureError
- [x] `set_api_key()` / `get_budget_status()` — Thin wrapper layer per RFC-0917 architecture note
- [x] `get_metrics()` — returns Prometheus dict with `quota_router_*` prefixed keys ✅
- [x] Model string parsing handles `provider/model` and `provider:model` (parse_model, parse_model_strict) — works correctly
- [x] Streaming uses actual SSE parsing (`parse_openai_sse`, `parse_anthropic_sse`) ✅
- [x] `cargo clippy -D warnings` passes ✅ (2026-05-08)
- [x] `cargo test --lib` passes ✅ (21 tests, 2026-05-08)
