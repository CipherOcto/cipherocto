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
4. Document ProviderBudgetLimiting disposition

## Acceptance Criteria

- [ ] `ProviderWithState.latencies: Vec<f64>` → `latencies: Vec<u64>` (integer microseconds, per-sample storage for sliding window)
- [ ] Add `avg_latency_us()` method that computes rolling average from samples (not stored separately)
- [ ] `ProviderWithState` add `success_count: u64, total_count: u64` fields
- [ ] `total_count` incremented on every `request_ended` call
- [ ] `success_count` incremented when `record_success()` is called (HTTP 2xx response)
- [ ] `request_ended` signature: `latency_ms: f64` → `latency_us: u64` (microseconds)
- [ ] `avg_latency()` removed (replaced by `avg_latency_us()` returning `u64`)
- [ ] `RouterConfig` add `weights: HashMap<String, u32>` — global model-name→weight map for Weighted strategy
- [ ] `Weighted` routing strategy added using global `weights` config (distinct from `SimpleShuffle`)
- [ ] `latency_based_impl` updated to call `avg_latency_us()` instead of `avg_latency()`
- [ ] `record_success(&mut self)` method added to `ProviderWithState` — increments `success_count`
- [ ] Success tracking is **external to Router**: router client calls `record_success()` after provider response succeeds, then calls `record_request_end()` to record latency
- [ ] `ProviderBudgetLimiting` disposition documented in code comment (out of scope per RFC-0902 v1.3)
- [ ] `Display` and `FromStr` updated for `Weighted` variant
- [ ] `cargo clippy --all-targets --all-features -- -D warnings` passes with zero warnings
- [ ] `cargo test --lib` passes

## Implementation Notes

### File Path
**Corrected:** `crates/quota-router-core/src/router.rs` (NOT `quota-router-cli`)

### Latency Storage Design (CRITICAL FIX)

The mission initially proposed storing only `avg_latency_us: u64` as a single aggregate. This is WRONG because it loses per-sample data needed for correct sliding window operations.

**Correct approach:** Store per-sample latencies as `Vec<u64>` (microseconds) and compute `avg_latency_us()` on demand:

```rust
pub struct ProviderWithState {
    pub provider: Provider,
    /// Current active requests (for LeastBusy)
    pub active_requests: u32,
    /// Rolling latency samples in microseconds (for LatencyBased)
    pub latencies: Vec<u64>,
    /// Success count (u64)
    pub success_count: u64,
    /// Total request count (u64)
    pub total_count: u64,
    pub current_rpm: u32,
    pub current_tpm: u32,
}

impl ProviderWithState {
    pub fn request_ended(&mut self, latency_us: u64, tokens: u32, latency_window: usize) {
        self.active_requests = self.active_requests.saturating_sub(1);
        self.latencies.push(latency_us);
        if self.latencies.len() > latency_window {
            self.latencies.drain(0..self.latencies.len() - latency_window);
        }
        self.current_rpm = self.current_rpm.saturating_add(1);
        self.current_tpm = self.current_tpm.saturating_add(tokens);
        self.total_count = self.total_count.saturating_add(1);
    }

    pub fn record_success(&mut self) {
        self.success_count = self.success_count.saturating_add(1);
    }

    pub fn avg_latency_us(&self) -> u64 {
        if self.latencies.is_empty() {
            u64::MAX // Very high latency for unproven providers
        } else {
            self.latencies.iter().sum::<u64>() / self.latencies.len() as u64
        }
    }
}
```

### Weighted vs SimpleShuffle

`Weighted` is semantically distinct from `SimpleShuffle`:
- `SimpleShuffle`: Weights derived from provider's rpm/tpm configuration (`get_routing_weight()`)
- `Weighted`: Weights explicitly configured via global `RouterConfig.weights` map (model_name → u32)

**RouterConfig needs this new field:**
```rust
pub struct RouterConfig {
    pub routing_strategy: RoutingStrategy,
    pub latency_window: usize,
    pub verbose: bool,
    /// Global weights map for Weighted strategy: model_name → weight
    /// Example YAML:
    ///   weights:
    ///     openai: 10
    ///     anthropic: 5
    pub weights: HashMap<String, u32>,
}
```

`Weighted` strategy implementation:
1. For each provider, look up `config.weights.get(provider.model_name)`
2. If found, use that weight
3. If not found, fall back to `get_routing_weight()` (rpm/tpm-derived)

This way `Weighted` can override specific providers while falling back to rpm-based behavior for others.

### success_count/total_count Increment Logic

- `total_count`: incremented on every `request_ended` call
- `success_count`: incremented only when `record_success()` is called (HTTP 2xx response)

**Call flow (SUCCESS case):**
```rust
// Router client (external to Router) receives successful provider response
router.get_provider(model_group, idx).unwrap().record_success(); // ← success_count++
router.record_request_end(model_group, idx, latency_us, tokens); // ← total_count++, latency recorded
```

**Call flow (FAILURE case):**
```rust
// Router client handles error — no record_success() call
// Failure is tracked separately; total_count is NOT incremented for failures
```

**Note:** The Router itself does NOT track success — it only tracks latency via `record_request_end()`. Success tracking is the router client's responsibility.

### API Breaking Change

`request_ended` signature changes from `latency_ms: f64` to `latency_us: u64`. All internal callers (internal `Router` methods) must convert before calling. This is a contained breaking change — no external callers exist outside this crate.

### latency_based_impl Update Required

`latency_based_impl` calls `avg_latency()` on line 284 of router.rs. When `avg_latency()` is removed and replaced by `avg_latency_us()`, update this call site:

```rust
// BEFORE (f64):
.min_by(|(_, a), (_, b)| {
    a.avg_latency()
        .partial_cmp(&b.avg_latency())
        .unwrap_or(std::cmp::Ordering::Equal)
})

// AFTER (u64):
.min_by_key(|(_, a)| a.avg_latency_us())
```

### ProviderBudgetLimiting Disposition

Add comment to code:
```rust
// ProviderBudgetLimiting is OUT OF SCOPE for this module.
// Per-provider budget limiting is handled by the budget enforcement layer (RFC-0904).
// CostBased routing selects lowest-cost provider but does not enforce per-provider budgets.
```
