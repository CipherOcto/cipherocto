# Mission: RFC-0917 Alignment — LatencyTracker u64 + QuotaRouterError

## Status

Open

## RFC

RFC-0917: Dual-Mode Query Router

**Note:** RFC-0917 is Draft, not Accepted. Mission created for planning and tracking purposes.

## Dependencies

- Mission: RFC-0902 Alignment (should complete first — shared routing types)

## Summary

Align RFC-0917 implementation with current spec changes:
1. Add `LatencyTracker` struct with u64 microseconds (integer, not f64)
2. Phase 3 `QuotaRouterError` — fully specified (R2-5 resolved), Phase 3 PLANNED items documented

## Acceptance Criteria

- [ ] `LatencyTracker` struct added with `record(provider: &str, latency_us: u64)` and `best_provider() -> Option<&str>` using integer u64 microseconds
- [ ] Remove duplicate `full` feature TOML block (was in RFC text, removed in v2.18)
- [ ] RouterError enum defined explicitly in RFC-0917 (R8-H1 fix: RateLimit, ProviderUnavailable, AuthError, ContentPolicyViolation, ContextWindowExceeded, Timeout, Unknown)
- [ ] SpendEvent construction fixed — request_id from req, pricing_hash from registry, token_source from tokenizer dispatch, all required fields present (XC-5 fix)
- [ ] A3 Router struct marked as non-normative pseudocode in code comments (added in v2.18)
- [ ] `cargo clippy --all-targets --all-features -- -D warnings` passes with zero warnings
- [ ] `cargo test --lib` passes

## Implementation Notes

**File:** `crates/quota-router-core/src/router.rs`

**LatencyTracker struct (RFC-0917):**
```rust
const LATENCY_WINDOW_SIZE: usize = 100;
struct LatencyTracker {
    samples: HashMap<String, Vec<u64>>,  // microseconds, integer
}
```
