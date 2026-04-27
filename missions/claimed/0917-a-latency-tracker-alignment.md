# Mission: RFC-0917 Alignment — LatencyTracker u64 + QuotaRouterError

## Status

Completed (2026-04-27)

## RFC

RFC-0917: Dual-Mode Query Router (Accepted v2.24)

## Dependencies

- Mission: RFC-0902 Alignment ✅ COMPLETED (archived)

## Summary

Align RFC-0917 implementation with current spec changes:
1. Add `LatencyTracker` struct with u64 microseconds (integer, not f64)
2. Phase 3 `QuotaRouterError` — fully specified (R2-5 resolved), Phase 3 PLANNED items documented

## Acceptance Criteria

- [x] `LatencyTracker` struct added with `record(provider: &str, latency_us: u64)` and `best_provider() -> Option<&str>` using integer u64 microseconds
- [x] Feature gate compile_error documented in code comments (feature flags deferred to Phase 2)
- [x] RouterError enum defined explicitly in RFC-0917 — already exists in `fallback.rs` (RateLimit, ProviderUnavailable, AuthError, ContentPolicyViolation, ContextWindowExceeded, Timeout, Unknown)
- [x] A3 Router struct marked as non-normative pseudocode in code comments (RFC-0917)
- [x] `cargo clippy --all-targets --all-features -- -D warnings` passes with zero warnings
- [x] `cargo test --lib` passes (161 tests)

**Note:** Phase 3 items (SpendEvent construction, full feature gates) are PLANNED per RFC-0917 §Phase 3.

## Implementation Notes

**File:** `crates/quota-router-core/src/router.rs`

**LatencyTracker struct (RFC-0917):**
```rust
const LATENCY_WINDOW_SIZE: usize = 100;
struct LatencyTracker {
    samples: HashMap<String, Vec<u64>>,  // microseconds, integer
}
```
