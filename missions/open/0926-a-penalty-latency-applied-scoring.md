# Mission: RFC-0926 — Penalty Latency Applied to Scoring

## Status

OPEN — ready to claim

## RFC

RFC-0926: Penalty Latency Applied to Scoring (Accepted v12)

## Dependencies

**Requires:**
- RFC-0925: Latency-Based Routing Extensions (CooldownTracker, penalty_latencies field)
- RFC-0917: Dual-Mode Query Router (LatencyTracker)

**Prerequisite implementations needed:**
- `CooldownTracker.penalty_latencies: Vec<u64>` must exist
- `CooldownTracker.record_timeout_penalty()` must be defined
- `CooldownTracker.clear_penalty_latencies()` must be defined
- `CooldownTracker.is_available()` must return correct availability

## Summary

Integrate penalty latencies stored in `CooldownTracker` into `LatencyTracker` scoring for latency-based routing decisions. When a deployment experiences timeouts or failures, penalty latencies (default 1_000_000_000µs per litellm) should be factored into provider selection to avoid routing to degraded deployments.

## Scope

### 1. CooldownTracker.get_penalty_latencies()

```rust
impl CooldownTracker {
    /// Get reference to penalty latencies for external query
    pub fn get_penalty_latencies(&self) -> &[u64] {
        &self.penalty_latencies
    }
}
```

### 2. LatencyTracker.best_provider_with_penalties()

New helper method on LatencyTracker that accepts penalty_map and available_names:

```rust
pub fn best_provider_with_penalties(
    &self,
    penalty_map: &std::collections::HashMap<String, Vec<u64>>,
    available_names: &std::collections::HashSet<&str>,
    is_streaming: bool,
) -> Option<(&str, f32)> {
    // Returns (provider_name, effective_latency) or None
    // TTFT-only for streaming with data (penalties NOT applied to TTFT)
    // Penalty-adjusted latency for non-streaming or streaming without TTFT
    // effective_latency = (sum(samples) + sum(penalties)) / (len(samples) + len(penalties))
}
```

### 3. Router Integration

Update `latency_based_with_cooldown_impl()` to:
1. Build `available_names` HashSet (providers not in cooldown)
2. Build `penalty_map` from `CooldownTracker.get_penalty_latencies()`
3. Call `best_provider_with_penalties()` when penalties exist, otherwise use `best_provider_among()`

## Implementation Checklist

**CooldownTracker changes:**
- [ ] `get_penalty_latencies()` — returns reference to penalty_latencies vector

**LatencyTracker changes:**
- [ ] `best_provider_with_penalties()` — new helper method with penalty-adjusted scoring

**Router integration:**
- [ ] `latency_based_with_cooldown_impl()` — builds penalty_map and calls best_provider_with_penalties()

**Testing:**
- [ ] Penalty increases effective latency ~100x for single timeout
- [ ] TTFT scoring ignores penalties (per design decision)
- [ ] Fresh deployments (no samples) return None

## Acceptance Criteria

- [ ] `CooldownTracker.get_penalty_latencies()` returns reference to penalties
- [ ] `best_provider_with_penalties()` returns `Option<(&str, f32)>` with correct scoring
- [ ] Penalty-adjusted latency for 99 samples @ 100ms + 1 penalty @ 1000s = ~10.1 seconds effective
- [ ] TTFT scoring ignores penalties (per RFC-0926 §TTFT Scoring)
- [ ] `cargo clippy -D warnings` passes
- [ ] `cargo test --lib` passes

## Key Files to Modify

| File | Change |
|------|--------|
| `crates/quota-router-core/src/router.rs` | Add get_penalty_latencies(), best_provider_with_penalties(), update latency_based_with_cooldown_impl() |

## Notes

**Single Source of Truth:** `CooldownTracker.penalty_latencies` is the single source. `LatencyTracker` does NOT store its own penalty latencies — it receives them via `best_provider_with_penalties()` parameter.

**Penalty Expiry:** Penalty latencies expire when cooldown expires. When `update_latency_state()` transitions to Healthy, it calls `clear_penalty_latencies()`.