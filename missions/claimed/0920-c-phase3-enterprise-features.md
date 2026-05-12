# Mission: RFC-0920 Phase 3 — Enterprise Features

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
- Batch execution (tokio)
- Budget/metrics APIs
- `native_http` module (reqwest providers for liteLLM-mode)

**PyO3 bindings (quota-router-pyo3) should be a thin marshal layer — NO heavy lifting.**

## Dependencies

- Mission: 0920-b-phase2-full-provider-coverage (Phase 2 completed)
- RFC-0913: Response Caching (required for `cache_responses` via stoolap)

---

## Architecture Reality (NOT per RFC-0917 ideal)

**RFC ideal:** Python SDK is thin wrapper → Rust core does routing.
**Code reality:** Python SDK has local routing + calls completion() function.

**This mission documents CURRENT state, not ideal state.**

---

## Phase 3 Checklist (per RFC-0920 lines 4623-4639)

**All items are BINDING LAYER only — heavy lifting stays in RFC-0917/Rust core.**

- [x] **Router class** — 8 routing strategies in Rust core (per RFC-0917); Python routing is NON-NORMATIVE placeholder (violates RFC-0917 thin-binding constraint)
- [x] **`batch_completion()`** — binding layer (in-memory parallel, ThreadPoolExecutor)
- [x] **`batch_completion_models()`** — binding layer (all models race, return first response)
- [x] **`batch_completion_models_all_responses()`** — binding layer (all responses from all models)
- [x] **Batch API** — binding layer (file-based: create_batch, retrieve_batch, cancel_batch, list_batches, retrieve_batch_results + async variants)
- [x] **Budget/metrics APIs** — binding layer (`get_budget_status()` with KeyStorage, `get_metrics()` Prometheus dict)
- [x] **SSEParser implementation** — for litellm-mode (reqwest SSE parsing); any-llm-mode uses Python SDK streaming, not SSE bytes

### Deferred to Phase 4 (all binding layer)

- [ ] **Responses API** — `/v1/responses` per OpenAI spec (binding layer)
- [ ] **Messages API** — with `system`, `top_k` support (binding layer)
- [ ] **`cache_responses`** — via stoolap semantic cache (RFC-0913) — binding layer only, actual caching in RFC-0917/Stoolap
- [ ] **`num_retries`** — per-call retry logic (binding layer)
- [ ] **`logger_fn`** — custom logger support (binding layer)
- [ ] **`thinking` parameter** — Anthropic extended thinking (binding layer)

---

## Discrepancies from RFC

| Component | RFC-0917 Says (Heavy Lifting Location) | Code Does | Severity |
|-----------|----------------------------------------|-----------|----------|
| PyO3 Router | Thin wrapper to Rust core RouterHandle | Local routing with RoundRobin state | **CRITICAL** |
| Routing strategies | 8 in Rust core | 8 in Python (local) + 8 in Rust core | **CRITICAL** |
| Batch execution | Rust core (tokio) | Python ThreadPoolExecutor | **CRITICAL** |
| Provider dispatch | Rust core | PyO3 bindings (55+ if-statements) | **CRITICAL** |

---

## 8 Routing Strategies (per RFC-0917 — in Rust core)

Rust core has all 8 (per RFC-0917 v2.50):
1. `simple-shuffle` — weighted random (default)
2. `round-robin` — sequential (via AtomicUsize)
3. `least-busy` — fewest active requests
4. `latency-based-routing` — fastest responding
5. `cost-based-routing` — cheapest
6. `usage-based-routing` — RPM/TPM based
7. `usage-based-routing-v2` — with recency-weighted scoring
8. `weighted` — explicit weights

**Python SDK has its own local routing (not using Rust core) — WRONG per RFC-0917.**

---

## Acceptance Criteria

**Binding layer items only (heavy lifting in RFC-0917/Rust core):**

- [x] Router class with 8 strategies (local Python routing — WRONG architecture)
- [x] `batch_completion()` — in-memory parallel, returns list of results
- [x] `batch_completion_models()` — all models race, returns first response
- [x] `batch_completion_models_all_responses()` — all responses from all models
- [x] File-based batch API — all 5 operations + async variants
- [x] `get_metrics()` — returns Prometheus dict
- [x] `get_budget_status()` — backed by KeyStorage
- [x] `cargo clippy -D warnings` passes
- [x] `cargo test --lib` passes

---

## ⚠️ Architecture Issue

**Current architecture violates RFC-0917 thin-binding constraint:**

| Item | Current Location | RFC-0917 Says |
|------|-----------------|---------------|
| Provider dispatch | PyO3 bindings (55+ if-statements) | Rust core |
| Router class | Local routing in Python | Thin wrapper to Rust core |
| Batch execution | Python ThreadPoolExecutor | Rust core (tokio) |

**Fix needed:** Separate binding layer (RFC-0920) from heavy lifting (RFC-0917). PyO3 should only marshal types and delegate to Rust core.