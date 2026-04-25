# Mission: RFC-0917 v2.18 Alignment — LatencyTracker u64 + QuotaRouterError

## Status

Open

## RFC

RFC-0917 v2.18 (Draft): Dual-Mode Query Router

**Note:** RFC-0917 is Draft, not Accepted. Mission created for planning and tracking purposes.

## Dependencies

- Mission: RFC-0902 v1.3 Alignment (should complete first — shared routing types)

## Summary

Align RFC-0917 implementation with RFC-0917 v2.18 changes:
1. Add `LatencyTracker` struct with u64 microseconds (integer, not f64)
2. Phase 3 `QuotaRouterError` — add to Phase 3 checklist, no enum implementation needed yet

## Acceptance Criteria

- [ ] `LatencyTracker` struct added with `record(provider: &str, latency_us: u64)` and `best_provider() -> Option<&str>` using integer u64 microseconds
- [ ] Remove duplicate `full` feature TOML block (was in RFC text, removed in v2.18)
- [ ] Phase 3 checklist items R2-5, R2-6, R2-7 documented as Phase 3 PLANNED
- [ ] A3 Router struct marked as non-normative pseudocode in code comments
- [ ] `cargo clippy --all-targets --all-features -- -D warnings` passes with zero warnings
- [ ] `cargo test --lib` passes

## Implementation Notes

**File:** `crates/quota-router-core/src/router.rs`

**LatencyTracker struct (RFC-0917 v2.18):**
```rust
const LATENCY_WINDOW_SIZE: usize = 100;
struct LatencyTracker {
    samples: HashMap<String, Vec<u64>>,  // microseconds, integer
}
```
