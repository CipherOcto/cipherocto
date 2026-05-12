# Mission: RFC-0920 Phase 2 — Full Provider Coverage

## Status

In Review — 2026-05-10

## RFC

RFC-0920: Unified Python SDK — Dual-Mode LiteLLM/any-llm Compatibility

## RFC-0920 Role: API Surface + Type Marshaling (Binding Layer)

**RFC-0920 is ONLY for API surface and type marshaling (binding layer).**

**RFC-0917 is the definitive source for ALL heavy lifting:**
- Routing strategies (8 strategies) in Rust core
- Provider dispatch logic
- State management (ProviderWithState, RouterState)
- `native_http` module (reqwest providers for liteLLM-mode)

**PyO3 bindings (quota-router-pyo3) should be a thin marshal layer — NO heavy lifting.**

## Dependencies

- Mission: 0920-a-phase1-core-sdk (Phase 1 completed)

---

## Architecture (per RFC-0917)

| Mode | Implementation | Provider Integration | Heavy Lifting Location |
|------|---------------|---------------------|------------------------|
| **any-llm-mode** (current) | `quota-router-pyo3` (Python binding) | PyO3 → official Python SDKs | ⚠️ WRONG: Heavy lifting in PyO3 bindings |
| **liteLLM-mode** | `quota-router-core` (Rust) | `reqwest` → provider REST APIs | ❌ NOT implemented |

---

## Phase 2 Checklist (per RFC-0920 lines 4613-4622)

**All items are BINDING LAYER only — no heavy lifting here.**

- [x] **Anthropic provider** — with `thinking` support (per RFC-0920 line 1342)
- [x] **Mistral provider** — integration
- [x] **All 42 providers** — via PyO3 → official Python SDKs
- [x] **Embedding API** — `embedding()` / `aembedding()`
- [x] **Model listing** — `list_models()` / `alist_models()` per RFC-0920 lines 4617
- [x] **`timeout` parameter** — `f64` seconds per spec
- [x] **`extra_headers`, `base_url`, `api_version` parameters** — per spec
- [x] **SSEParser implementation** — `parse_openai_sse`, `parse_anthropic_sse` (litellm-mode only)

---

## 42 Providers (any-llm-mode via PyO3)

```
anthropic, azure, azureanthropic, azureopenai, bedrock, cerebras, cohere,
dashscope, databricks, deepinfra, deepseek, fireworks, gateway, gemini, groq,
huggingface, inception, llama, llamacpp, llamafile, lmstudio, minimax, mistral,
moonshot, mzai, nebius, ollama, openai, openrouter, perplexity, platform, portkey,
sagemaker, sambanova, together, vertexai, vertexaianthropic, vllm, voyage,
watsonx, xai, zai
```

---

## Discrepancy Note

**SSEParser functions — not a gap, different mode:**

`parse_openai_sse` and `parse_anthropic_sse` in `streaming.rs` are for **parsing SSE bytes** (litellm-mode use case: reqwest receives SSE bytes from provider REST API).

For **any-llm-mode**, the streaming path is different:
1. Python SDK returns Python streaming objects (not SSE bytes)
2. PyO3 bridge converts Python objects → OpenAI-compatible SSE format
3. SSE bytes are streamed through HTTP response

So `parse_openai_sse`/`parse_anthropic_sse` are **correctly unused in any-llm-mode** — we're not parsing SSE bytes, we're creating SSE from Python objects.

**RFC spec for any-llm streaming (lines 3260-3288):**
- Python SDK streaming APIs return Python objects
- PyO3 bridge converts to OpenAI SSE format via `AsyncChunkIterator` (spawn_blocking + oneshot)
- `#[allow(dead_code)]` on parse functions is appropriate — these are for litellm-mode, not any-llm-mode

---

## Acceptance Criteria

- [x] All 42 providers accessible via any-llm-mode SDK (mock OK where no Python SDK available)
- [x] `embedding()` / `aembedding()` — correct signature per RFC-0920
- [x] `list_models()` / `alist_models()` — correct signature per RFC-0920 lines 4617
- [x] `timeout` parameter — signature present per spec
- [x] `cargo clippy -D warnings` passes
- [x] `cargo test --lib` passes

---

## LiteLLM Mode Note

LiteLLM-mode provider integration (Rust `reqwest`) is a separate effort and NOT in scope for Phase 2.

**Heavy lifting for liteLLM-mode (`native_http` reqwest providers) belongs in RFC-0917 core crate — not in PyO3 binding layer.**

---

## ⚠️ Architecture Issue

**Current architecture violation (RFC-0917 thin-binding constraint):**

PyO3 bindings (`quota-router-pyo3/src/completion.rs`) contain:
- 55+ provider if-statements doing provider dispatch (HEAVY LIFTING)
- Direct Python SDK initialization and calls

**Per RFC-0917:** PyO3 bindings should be thin marshal layer only. Provider dispatch belongs in Rust core.

This is NOT a Phase 2 gap — it's a fundamental architecture issue that requires separating binding layer (RFC-0920) from heavy lifting (RFC-0917). Future work: refactor PyO3 to delegate provider dispatch to Rust core.