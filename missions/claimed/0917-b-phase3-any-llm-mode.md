# Mission: RFC-0917 Phase 3 — any-llm Mode Implementation

## Status

In Progress — 20 API functions + 16 exceptions implemented, provider integrations remaining (2026-04-27)

## RFC

RFC-0917: Dual-Mode Query Router (Accepted v2.24)

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

## Phase 3 Checklist (RFC-0917 lines 997-1007) — EXPANDED

- [ ] **PyO3 bridge** — quota-router-pyo3 crate calls official Python SDKs via PyO3
- [ ] **41 Provider integrations** via PyO3 calls to: `anthropic`, `openai`, `mistralai`, `ollama`, `google-genai` + 36 more
- [ ] **Python SDK package** (`pip install quota-router` or `quota_router`)
- [x] **20 API functions** via PyO3: completion/acompletion, responses/aresponses, messages/amessages, embedding/aembedding, list_models/alist_models, batch operations (all 20 implemented as mocks)
- [ ] **Streaming** via PyO3 (Python async generators)
- [x] **any-llm-compatible exceptions** (all 16 exceptions implemented in exceptions.rs)
- [ ] `set_api_key()` — validates and registers key with storage
- [ ] `get_budget_status()` — returns current spend vs limit
- [ ] `get_metrics()` — returns Prometheus metrics dict
- [ ] **Model string parsing** (`provider/model` and `provider:model` formats)
- [x] **QuotaRouterError** — spec done; From impls + Error traits needed

**Note:** All 20 API functions and 16 exceptions are implemented as mocks/stubs. Actual provider integrations, streaming, and storage integration remain to be implemented.

## Acceptance Criteria

- [x] quota-router-pyo3 implements all 20 API functions via PyO3 (all 20 implemented as mocks)
- [ ] All 41 providers accessible via any-llm-mode
- [x] Exception hierarchy matches any-llm's AnyLLMError hierarchy (all 16 implemented)
- [ ] `set_api_key()`, `get_budget_status()`, `get_metrics()` implemented
- [ ] Streaming via PyO3 async generators
- [ ] Model string parsing handles `provider/model` and `provider:model`
- [x] `QuotaRouterError` with From impls and Error trait impls for all wrapped types
- [x] `cargo clippy -D warnings` and `cargo test --lib` pass
