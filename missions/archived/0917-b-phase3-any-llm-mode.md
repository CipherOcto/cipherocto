# Mission: RFC-0917 — any-llm Mode Implementation

## Status

COMPLETE — all acceptance criteria met

## RFC

RFC-0917: Dual-Mode Query Router

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

## Architecture (per RFC-0917 §Module Structure)

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

## Implementation Checklist (per RFC-0917 §Phase 3 Implementation)

**MUST be in `quota-router-core`:**

- [x] **`py_bridge` module** — PyO3 → official Python SDKs (INTERNAL boundary #1) ✅
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

## liteLLM Mode — Covered by Mission 0917-c

Per RFC-0917 §native_http Module, the `native_http` module provides liteLLM Mode via reqwest. Mission 0917-c covers liteLLM mode implementation.

---

## Acceptance Criteria

**Rust Core (`quota-router-core`):**

- [x] `py_bridge` module with 42 provider integrations via PyO3 (build passes)
- [x] `python_sdk_entry` module with `completion`, `set_api_key`, `get_budget_status`, `get_metrics`
- [x] `set_api_key()` delegates to `KeyStorage` trait (via `STORAGE.create_provider_key()`)
- [x] `get_budget_status()` delegates to `KeyStorage` (via `STORAGE.list_provider_keys()`)
- [x] `get_metrics()` delegates to core (via `STORAGE.list_provider_keys()` → Prometheus dict)
- [x] Provider dispatch in `python_sdk_entry::completion()` uses `py_bridge::factory::completion()`

**PyO3 Binding (`quota-router-pyo3` — thin ONLY):**

- [x] Type marshaling only (no business logic)
- [x] `set_api_key()`, `get_budget_status()`, `get_metrics()` are thin wrappers (in-memory fallback for interface parity)

**Build Verification (2026-05-11):**

- [x] `cargo build -p quota-router-core --no-default-features --features any-llm-mode` — PASS
- [x] `cargo clippy -p quota-router-core --no-default-features --features any-llm-mode -- -D warnings` — 0 warnings
- [x] `cargo test -p quota-router-core --lib --no-default-features --features any-llm-mode` — 166 tests pass
- [x] `cargo build -p quota-router-pyo3` — PASS
- [x] `cargo build -p quota-router-core --features full` — PASS
- [x] `cargo clippy -p quota-router-core --features full -- -D warnings` — 0 warnings
- [x] `cargo fmt -- --check` — clean (0 diff)
