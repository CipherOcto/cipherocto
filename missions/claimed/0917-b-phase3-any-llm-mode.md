# Mission: RFC-0917 Phase 3 — any-llm Mode Implementation

## Status

PHASE 3 IMPLEMENTATION IN PROGRESS — py_bridge providers created in core

## RFC

RFC-0917: Dual-Mode Query Router (Accepted v2.50)

## RFC-0917 Role: Heavy Lifting (Rust Core)

**RFC-0917 is the definitive source for ALL heavy lifting:**
- Routing strategies (8 strategies) — in Rust core
- Provider dispatch logic — in Rust core
- State management (ProviderWithState, RouterState) — in Rust core
- Batch execution (tokio) — in Rust core
- Budget/rate limiting — in Rust core
- Cache management — in Rust core
- **`py_bridge` module** — PyO3 → official Python SDKs (INTERNAL boundary #1) — in Rust core
- **`python_sdk_entry` module** — PyO3 entry point (EXTERNAL boundary #2) — in Rust core
- `native_http` module (reqwest providers for liteLLM-mode) — in Rust core

**RFC-0920 is ONLY for API surface and type marshaling (binding layer).**

## Architecture (per RFC-0917 lines 293-297)

RFC-0917 explicitly defines two internal boundaries for any-llm-mode:

```rust
// INTERNAL boundary #1 — core → provider SDKs (MUST be in quota-router-core)
#[cfg(any(feature = "any-llm-mode", feature = "full"))]
pub mod py_bridge;    // PyO3 → official Python SDKs

// EXTERNAL boundary #2 — pyo3 → core (MUST be in quota-router-core)
#[cfg(any(feature = "any-llm-mode", feature = "full"))]
pub mod python_sdk_entry;  // PyO3 entry point
```

**Correct call flow:**
```
Python SDK (quota-router-pyo3)
    ↓ calls
python_sdk_entry (EXTERNAL boundary #2 — in quota-router-core)
    ↓ calls
py_bridge (INTERNAL boundary #1 — in quota-router-core)
    ↓ PyO3 calls
Official Python SDKs (Anthropic, OpenAI, Mistral, etc.)
```

**pyo3 crate role:** Only type marshaling + calls into `python_sdk_entry`. NO business logic.

---

## Current State (per RFC-0917)

| Component | RFC-0917 Says | Currently In |
|-----------|---------------|--------------|
| `py_bridge` module | `quota-router-core` | ✅ `quota-router-core/src/py_bridge/` (42 providers) |
| `python_sdk_entry` module | `quota-router-core` | ✅ `quota-router-core/src/python_sdk_entry/` |
| Provider SDK calls (openai, anthropic, etc.) | `quota-router-core/src/py_bridge/` | ✅ Correct location |
| `set_api_key()` | `quota-router-core` (KeyStorage) | ✅ `python_sdk_entry/sdk_functions.rs` → `STORAGE.create_provider_key()` |
| `get_budget_status()` | `quota-router-core` (KeyStorage) | ✅ `python_sdk_entry/sdk_functions.rs` → `STORAGE.list_provider_keys()` |
| `get_metrics()` | `quota-router-core` (KeyStorage) | ✅ `python_sdk_entry/sdk_functions.rs` → `STORAGE.list_provider_keys()` → Prometheus dict |
| Provider dispatch | `py_bridge::factory::completion()` | ✅ `python_sdk_entry/completion.rs` delegates to factory |

---

## Phase 3 Checklist (per RFC-0917 lines 2709-2719)

**MUST be in `quota-router-core`:**

- [x] **`py_bridge` module** — PyO3 → official Python SDKs (INTERNAL boundary #1) ✅ (42 providers created)
- [x] **`python_sdk_entry` module** — PyO3 entry point (EXTERNAL boundary #2) ✅ (completion.rs + sdk_functions.rs)
- [x] **`set_api_key()`** — delegates to core `KeyStorage` via `STORAGE.create_provider_key()` ✅
- [x] **`get_budget_status()`** — returns provider keys via `STORAGE.list_provider_keys()` ✅
- [x] **`get_metrics()`** — returns Prometheus dict via `STORAGE` ✅
- [x] **Provider dispatch** — `completion.rs` calls `py_bridge::factory::completion()` ✅

**Correctly in `quota-router-pyo3` (thin binding ONLY):**

- [x] **Type marshaling** — Python dict ↔ Rust types (in types.rs, model.rs)
- [x] **Model parsing** — `"provider/model"` → `ParsedModel` (in model.rs)
- [x] **PyO3 wrapper** — thin wrapper (sdk.rs has in-memory fallback for pyo3 interface parity)

**Storage infrastructure (core):**
- [x] `provider_api_keys` table in schema.rs
- [x] `ProviderKeyInfo` struct in storage.rs
- [x] `create_provider_key`, `list_provider_keys`, `delete_provider_key`, `get_provider_key_by_hash` methods in storage.rs
- [x] `STORAGE` global singleton in storage.rs

---

## liteLLM Mode — NOT IMPLEMENTED (RFC-0917 SPEC EXISTS)

**RFC-0917 lines 291, 340-369, 1833, 1855, 1861 define `native_http` module:**

```
quota-router-core/src/native_http/
├── mod.rs              # HttpProvider trait
├── openai.rs           # OpenAI via reqwest
├── anthropic.rs        # Anthropic via reqwest
...
```

This is a separate mission — belongs in `quota-router-core`.

---

## Acceptance Criteria

**Rust Core (`quota-router-core`):**

- [x] `py_bridge` module with 42 provider integrations via PyO3 (all created, build passes)
- [x] `python_sdk_entry` module with `completion`, `set_api_key`, `get_budget_status`, `get_metrics`
- [x] `set_api_key()` delegates to `KeyStorage` trait (via `STORAGE.create_provider_key()`)
- [x] `get_budget_status()` delegates to `KeyStorage` (via `STORAGE.list_provider_keys()`)
- [x] `get_metrics()` delegates to core (via `STORAGE.list_provider_keys()` → Prometheus dict)
- [x] Provider dispatch in `python_sdk_entry::completion()` uses `py_bridge::factory::completion()`

**PyO3 Binding (`quota-router-pyo3` — thin ONLY):**

- [x] Type marshaling only (no business logic)
- [x] `set_api_key()`, `get_budget_status()`, `get_metrics()` are thin wrappers (in-memory fallback for interface parity)

- [x] `cargo clippy -p quota-router-core` passes (warnings only)
- [x] `cargo test -p quota-router-core --lib` passes (161 tests)
- [x] `cargo build -p quota-router-pyo3` passes
