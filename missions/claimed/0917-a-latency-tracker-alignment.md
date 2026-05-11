# Mission: RFC-0917 Alignment — LatencyTracker u64 + QuotaRouterError

## Status

COMPLETED (2026-05-10)

## RFC

RFC-0917: Dual-Mode Query Router (Accepted v2.50)

## RFC-0917 Role: Heavy Lifting (Rust Core)

**RFC-0917 is the definitive source for ALL heavy lifting:**
- Routing strategies (8 strategies)
- Provider dispatch logic
- State management (ProviderWithState, RouterState)
- Request/response processing
- Budget and rate limiting
- Cache management
- `native_http` module (reqwest providers for liteLLM-mode)

**RFC-0920 is ONLY for API surface and type marshaling (binding layer).**

## Dependencies

- Mission: RFC-0902 Alignment ✅ COMPLETED (archived)

## Summary

Align RFC-0917 implementation with current spec changes:
1. Add `LatencyTracker` struct with u64 microseconds (integer, not f64)
2. Phase 3 `QuotaRouterError` — fully specified (R2-5 resolved), Phase 3 PLANNED items documented

## Acceptance Criteria

- [x] `LatencyTracker` struct added with `record(provider: &str, latency_us: u64)` and `best_provider() -> Option<&str>` using integer u64 microseconds
- [x] A3 Router struct marked as non-normative pseudocode in code comments (RFC-0917)
- [x] RouterError enum defined explicitly in RFC-0917 — already exists in `fallback.rs` (RateLimit, ProviderUnavailable, AuthError, ContentPolicyViolation, ContextWindowExceeded, Timeout, Unknown)
- [x] `cargo clippy --all-targets --all-features -- -D warnings` passes with zero warnings
- [x] `cargo test --lib` passes (161 tests)
- [x] `QuotaRouterError` unified error type (wraps KeyError, BudgetError, RouterError, RegistryError, StorageError + ProviderError variant)
- [x] Feature gate compile_error (requires litellm-mode/any-llm-mode/full features in Cargo.toml)

**Note:** Phase 3 items (PyO3 bridge, Provider SDK integrations, Python SDK interface, streaming, etc.) are PLANNED per RFC-0917 §Phase 3 — not yet due.

## Implementation Notes

**File:** `crates/quota-router-core/src/router.rs`

**LatencyTracker struct (RFC-0917):**
```rust
const LATENCY_WINDOW_SIZE: usize = 100;
struct LatencyTracker {
    samples: HashMap<String, VecDeque<u64>>,  // microseconds, integer, O(1) eviction
}
```

**Key implementation detail:** Uses `VecDeque` for O(1) front eviction instead of `Vec.remove(0)` which is O(n).

## Completion Notes

### QuotaRouterError (2026-04-27)

Added unified error type in `crates/quota-router-core/src/keys/errors.rs`:
- Wraps KeyError, BudgetError, RouterError, RegistryError, StorageError, ProviderError
- KeyError and BudgetError derive Clone to support QuotaRouterError::Clone

### Feature Gate Compile Error (2026-04-27)

Added RFC-0917 §Rust Feature Gates compile_error to `crates/quota-router-core/src/router.rs`:
```rust
#[cfg(all(feature = "litellm-mode", feature = "any-llm-mode"))]
compile_error!("Cannot enable both 'litellm-mode' and 'any-llm-mode' — they are mutually exclusive per RFC-0917 §Rust Feature Gates");
```

Feature gates in Cargo.toml:
- `litellm-mode` (default): hyper, axum, tokio, etc.
- `any-llm-mode`: empty marker only; PyO3 bindings live in quota-router-pyo3 crate
- `full`: enables all litellm-mode dependencies (superset of litellm-mode)
- compile_error fires when both litellm-mode and any-llm-mode are enabled simultaneously