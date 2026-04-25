# Mission: RFC-0902 v1.3 Alignment — Integer Metrics + Weighted Strategy

## Status

Open

## RFC

RFC-0902 v1.3 (Accepted): Multi-Provider Routing and Load Balancing

## Dependencies

None (can proceed independently)

## Summary

Update `crates/quota-router-core/src/router.rs` to match RFC-0902 v1.3 changes:
1. Replace f64 latency tracking with u64 microseconds in ProviderWithState
2. Add success_count/total_count u64 metrics
3. Add Weighted routing strategy (7th strategy, currently missing)
4. Document ProviderBudgetLimancing disposition

## Acceptance Criteria

- [ ] `ProviderWithState.latencies: Vec<f64>` → `avg_latency_us: u64` (integer microseconds, rolling average computed as integer)
- [ ] `ProviderWithState` add `success_count: u64, total_count: u64` fields (success_rate removed, ratio computed at display time only)
- [ ] `request_ended()` and `avg_latency()` methods updated to use integer microseconds
- [ ] `Weighted` routing strategy added (7th strategy, was 6)
- [ ] ProviderBudgetLimiting disposition documented in code comment (out of scope per RFC-0902 v1.3)
- [ ] `cargo clippy --all-targets --all-features -- -D warnings` passes with zero warnings
- [ ] `cargo test --lib` passes

## Implementation Notes

**File:** `crates/quota-router-core/src/router.rs`

**Current (v1.3 mismatch):**
```rust
pub struct ProviderWithState {
    pub provider: Provider,
    pub active_requests: u32,
    pub latencies: Vec<f64>,        // f64 milliseconds
    pub current_rpm: u32,
    pub current_tpm: u32,
}
```

**Required (RFC-0902 v1.3):**
```rust
pub struct ProviderWithState {
    pub provider: Provider,
    pub active_requests: u32,
    /// Rolling average latency in microseconds (integer)
    pub avg_latency_us: u64,
    pub success_count: u64,
    pub total_count: u64,
    pub current_rpm: u32,
    pub current_tpm: u32,
}
```

**LatencyTracker (RFC-0917):** RFC-0917 v2.18 defines a separate `LatencyTracker` struct with u64 microseconds. This mission aligns RFC-0902's ProviderWithState. RFC-0917 may need separate alignment if LatencyTracker is distinct from ProviderWithState.
