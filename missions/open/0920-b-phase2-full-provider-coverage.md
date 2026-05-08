# Mission: RFC-0920 Phase 2 — Full Provider Coverage

## Status

Open — depends on Phase 1

## RFC

RFC-0920 (Economics): Unified Python SDK — Dual-Mode LiteLLM/any-llm Compatibility (Accepted v1.58)

## Dependencies

- Mission: 0920-a-phase1-core-sdk (Phase 1 must complete first)

## Context

Phase 2 adds all 41 providers and completes the API surface. Mock until real SDK available, but interface must be correct.

**H1/H4 fix:** RFC-0920 header says "41 providers" — deepinfra IS in the list (making 41 total), not an extra 42nd provider.

## Phase 2 Checklist (RFC-0920 lines 4613-4622)

- [ ] **Anthropic provider** — with `thinking` support (per RFC-0920 line 1342); **cache_control is TODO** per RFC-0920 line 4615 — not specced yet (B3)
- [ ] **Mistral provider** — integration
- [ ] **All 41 providers** — mock until real SDK available (per RFC list)
  - **B7 note:** Not all 41 providers have official Python SDKs. Some (llama.cpp, llamafile, ollama, local inference) require custom handling.
- [ ] **Embedding API** — `embedding()` / `aembedding()`
- [ ] **Model listing** — `list_models()` / `alist_models()` (alist_models B2 gap: not in RFC-0920)
- [ ] **`timeout` parameter** — `httpx.Timeout` support (per LiteLLM signature)
- [ ] **`extra_headers`, `base_url`, `api_version` parameters** — per spec
  - **B8 note:** `base_url` is alias for `api_base` (RFC-0920 line 363). Embedding signature uses `api_base`, not `base_url`.
- [ ] **SSEParser implementation** — Phase 2 SSE parsing per RFC-0920 lines 1498, 1991, 2033 **H5 fix: added**

**H2 fix:** `responses()/aresponses()` is Phase 3 per RFC-0920 line 4619 — removed from Phase 2 checklist.

## 41 Providers (RFC-0920 lines 531-798)

```
anthropic, azure, azureanthropic, azureopenai, bedrock, cerebras, cohere,
dashscope, databricks, deepinfra, deepseek, fireworks, gateway, gemini, groq,
huggingface, inception, llama, llamacpp, llamafile, lmstudio, minimax, mistral,
moonshot, mzai, nebius, ollama, openai, openrouter, perplexity, platform, portkey,
sagemaker, sambanova, together, vertexai, vertexaianthropic, vllm, voyage,
watsonx, xai, zai
```

**H1/H4 fix:** Count is 41 (deepinfra is in the list). RFC-0920 header says 41. Previously mission said "42" which was incorrect.

## Acceptance Criteria

- [ ] All 41 providers accessible via SDK (mock OK for Phase 2)
- [ ] `embedding()` / `aembedding()` — correct signature per RFC-0920
- [ ] `list_models()` / `alist_models()` — **M2 fix:** `alist_models` NOT in RFC-0920 (implementation gap); only `list_models` is specced
- [ ] `timeout` parameter — `httpx.Timeout` per spec
- [ ] `cargo clippy -D warnings` passes
- [ ] `cargo test --lib` passes