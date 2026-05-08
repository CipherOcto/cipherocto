# Mission: RFC-0920 Phase 3 — Enterprise Features

## Status

Open — depends on Phase 2

## RFC

RFC-0920 (Economics): Unified Python SDK — Dual-Mode LiteLLM/any-llm Compatibility (Accepted v1.58)

## Dependencies

- Mission: 0920-b-phase2-full-provider-coverage (Phase 2 must complete first)
- RFC-0913: Response Caching (required for `cache_responses` via stoolap)

## Context

Phase 3 adds the Router class, batch APIs, and advanced features. The Router class is a thin PyO3 wrapper delegating to `quota-router-core` — no Python-side routing state.

## Phase 3 Checklist (RFC-0920 lines 4623-4639)

- [ ] **Router class** — 8 routing strategies per RFC-0902 (simple-shuffle, round-robin, least-busy, latency-based, cost-based, usage-based, usage-based-v2, weighted)
- [ ] **`batch_completion()`** — in-memory parallel batch (ThreadPoolExecutor)
- [ ] **`batch_completion_models()`** — all models race, **return first response** (wins race) **J1 fix: was "return list"**
- [ ] **`batch_completion_models_all_responses()`** — all responses from all models **J3 fix: standalone function, not Router method**
- [ ] **Batch API** — file-based (create_batch, retrieve_batch, cancel_batch, list_batches, retrieve_batch_results + async variants)
  - **C1 NOTE:** RFC-0920 does NOT define file-based batch operations for quota-router. Only in-memory `batch_completion()` (lines 2207-2236) is specced. File-based batch requires RFC-0920 spec extension OR this item should be removed. Currently marked as TODO in RFC-0920 line 2197.
- [ ] **Responses API** — `/v1/responses` per OpenAI spec
- [ ] **Messages API** — with `system`, `top_k` support
  - **E2 fix:** `truncation` is NOT in Messages API signature — it's Phase 4 (per RFC-0920 lines 1159, 4629)
- [ ] **Budget/metrics APIs** — `get_budget_status()` backed by StoolapKeyStorage, `get_metrics()` Prometheus dict
- [ ] **`cache_responses`** — via **stoolap** semantic cache (RFC-0913), NOT Redis
- [ ] **`num_retries`** — per-call retry logic
- [ ] **`logger_fn`** — custom logger support
- [ ] **Exception regex mapping** — `QUOTA_ROUTER_UNIFIED_EXCEPTIONS=1` mode
- [ ] **Platform provider** — any-api key format (J4 fix: already in 41 providers; this entry documents any-api format specifically)
- [ ] **`thinking` parameter** — Anthropic extended thinking
- [ ] **`safety_identifier`** — per LiteLLM signature
- [ ] **SSE normalization pipeline** — provider-specific SSE parsing per RFC-0920 lines 2088-2154 (Phase 3)
  - **J5 fix:** SSEParser class (lines 1689-1900) is Phase 2 implementation. Phase 3 normalization pipeline (lines 2088-2154) uses SSEParser for transformations.

## Key Router Methods (RFC-0920 lines ~2582+ for Router class)

**E1/E3 fix:** Router class is a thin wrapper around quota-router-core. Actual Router methods (per RFC-0920):
- `Router.completion()` — line 2898
- `Router.acompletion()` — line 3019
- `Router.list_models()` — lines 1029, 1055
- `Router.get_metrics()` — lines ~2582+ (Router class definition) **J2 fix: corrected line reference**

**Standalone batch functions** (NOT Router methods, separate per RFC-0920):
- `batch_completion()` — lines 2207-2261 (ThreadPoolExecutor)
- `batch_completion_models()` — lines 2359 (first response wins)
- `batch_completion_models_all_responses()` — lines 2424 (all responses) **J3 fix: not a Router method**

## 8 Routing Strategies (per RFC-0902, RFC-0920 lines 2642-2649)

1. `simple-shuffle` — weighted random (default)
2. `round-robin` — sequential
3. `least-busy` — fewest active requests
4. `latency-based-routing` — fastest responding
5. `cost-based-routing` — cheapest
6. `usage-based-routing` — RPM/TPM based
7. `usage-based-routing-v2` — with recency-weighted scoring
8. `weighted` — explicit weights

## Acceptance Criteria

- [ ] Router class with all 8 strategies — thin PyO3 wrapper, no Python routing state
- [ ] `batch_completion()` — in-memory parallel, returns list of results
- [ ] `batch_completion_models()` — all models race, returns **first response** (wins race) **J1 fix: not a list**
- [ ] `batch_completion_models_all_responses()` — all responses from all models
- [ ] File-based batch API — all 5 operations + async variants
  - **C1 WARNING:** This item requires RFC-0920 spec extension — file-based batch is NOT currently defined for quota-router
- [ ] Responses API — correct signature per RFC-0920
- [ ] Messages API — with system, top_k params **E2 fix: removed truncation (Phase 4)**
- [ ] `get_metrics()` — returns Prometheus dict (not in-memory counter)
- [ ] `get_budget_status()` — backed by StoolapKeyStorage per checklist
- [ ] `cargo clippy -D warnings` passes
- [ ] `cargo test --lib` passes