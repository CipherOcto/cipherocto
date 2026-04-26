# Mission: RFC-0902 v1.6 Alignment — Integer Metrics + Weighted Strategy

## Status

Open

## RFC

RFC-0902 v1.6 (Accepted): Multi-Provider Routing and Load Balancing

## Dependencies

None (can proceed independently)

## Summary

Update `crates/quota-router-core/src/router.rs` to match RFC-0902 v1.5 changes:
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
- [ ] `RouterConfig` add `weights: HashMap<String, u32>` — global provider-name→weight map for Weighted strategy
- [ ] `Weighted` routing strategy added using global `weights` config (distinct from `SimpleShuffle`)
- [ ] `Weighted` requires new `weighted_impl(providers, weights) -> usize` method (simple_shuffle_impl has no access to config.weights)
- [ ] `Weighted` added to `route()` match: `RoutingStrategy::Weighted => Self::weighted_impl(providers, &self.config.weights)`
- [ ] `latency_based_impl` updated to call `avg_latency_us()` instead of `avg_latency()`
- [ ] `record_success(&mut self)` method added to `ProviderWithState` — increments `success_count`
- [ ] Success tracking is **external to Router**: router client calls `record_success()` after provider response succeeds, then calls `record_request_end()` to record latency
- [ ] `ProviderBudgetLimiting` disposition documented in code comment (out of scope per RFC-0902 v1.5)
- [ ] `Display` and `FromStr` updated for `Weighted` variant
- [ ] `Default` impl for `RouterConfig` updated to initialize `weights: HashMap::new()`
- [ ] Tests updated: all `vec![f64]` latency literals → `vec![u64]` microseconds; `record_request_end(..., f64, ...)` → `record_request_end(..., u64, ...)`
- [ ] Test for `Weighted` strategy: `"weighted".parse::<RoutingStrategy>()` round-trip test added
- [ ] Tests for `success_count`/`total_count`: `record_success()` and `request_ended()` behavior verified
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
    /// Global weights map for Weighted strategy: provider.name → weight
    /// Example YAML:
    ///   weights:
    ///     openai: 10
    ///     anthropic: 5
    pub weights: HashMap<String, u32>,
}
```

`Weighted` strategy implementation:
1. For each provider, look up `config.weights.get(provider.name)` — keyed by **provider name**, not model_name
2. If found, use that weight
3. If not found, fall back to `get_routing_weight()` (rpm/tpm-derived)

**Why provider.name not model_name:** The weights map is a global override per-provider. Different providers (e.g., "openai" and "azure") sharing the same model group can have different weights. Using model_name as the key would give the same weight to all providers in a model group, which doesn't allow fine-grained control.

```rust
fn weighted_impl(providers: &[ProviderWithState], weights: &HashMap<String, u32>) -> usize {
    // Build weight list: global override or fallback to get_routing_weight()
    let weights: Vec<u32> = providers.iter().map(|p| {
        weights.get(&p.provider.name).copied().unwrap_or_else(|| p.get_routing_weight())
    }).collect();
    // Then do weighted random selection (same as simple_shuffle_impl)
    ...
}
```

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
// Router client handles error — record_success() NOT called
// BUT record_request_end() IS still called to track failure latency
router.record_request_end(model_group, idx, latency_us, tokens); // ← total_count++ (success_count unchanged)
```

**Note:** Failures still call `record_request_end()` to track latency (e.g., timeout latencies). This provides visibility into failure behavior. `total_count` increments but `success_count` stays unchanged — giving an accurate success rate when computed as `success_count / total_count`.

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

### Weighted Implementation Requires New Method

`simple_shuffle_impl` takes only `&[ProviderWithState]` — it has **no access to `RouterConfig.weights`**. `Weighted` cannot reuse `simple_shuffle_impl` directly.

**Add new method to Router:**
```rust
/// Weighted strategy: uses global weights map (provider.name → weight), falls back to get_routing_weight()
fn weighted_impl(providers: &[ProviderWithState], weights: &HashMap<String, u32>) -> usize {
    let weights: Vec<u32> = providers.iter().map(|p| {
        weights.get(&p.provider.name).copied().unwrap_or_else(|| p.get_routing_weight())
    }).collect();
    // Use same weighted random selection logic as simple_shuffle_impl
    Self::weighted_random(providers, weights)
}

fn weighted_random(providers: &[ProviderWithState], weights: Vec<u32>) -> usize {
    let total_weight: u32 = weights.iter().sum();
    if total_weight == 0 {
        rand::rng().random_range(0..providers.len())
    } else {
        let mut cumulative = 0u32;
        let weighted: Vec<u32> = weights.iter().map(|&w| { cumulative += w; cumulative }).collect();
        let roll = rand::rng().random_range(1..=total_weight);
        weighted.iter().position(|&w| w >= roll).unwrap_or(0)
    }
}
```

**Add to `route()` match:**
```rust
RoutingStrategy::Weighted => Self::weighted_impl(providers, &self.config.weights),
```

### ProviderBudgetLimiting Disposition

Add comment to code:
```rust
// ProviderBudgetLimiting is OUT OF SCOPE for this module.
// Per-provider budget limiting is handled by the budget enforcement layer (RFC-0904).
// CostBased routing selects lowest-cost provider but does not enforce per-provider budgets.
```
