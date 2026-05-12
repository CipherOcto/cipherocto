# Mission: RFC-0917 Phase 2 — LatencyTracker Integration

## Status

SPECIFIED — ready for implementation (this is the spec, not implementation)

## RFC

RFC-0917: Dual-Mode Query Router

## Phase 2 Context

Phase 1 (RFC-0917 acceptance) and Phase 3 (missions a, b, c, d) established:
- `LatencyTracker` struct with `record()` and `best_provider_with_ttft()` (per RFC-0925)
- `ProviderWithState.latencies` Vec for per-provider sliding window
- `LatencyBased` routing strategy using `avg_latency_us()`
- Stub comment: "Phase 2: LatencyTracker will be integrated into RouterState"

**The problem:** LatencyTracker is standalone but not wired into Router's routing flow. Phase 2 must specify what "integration" means.

## Scope

### 1. LatencyTracker Role Clarification

**Current state (Phase 3):**
- `LatencyTracker` struct exists but is never called
- `ProviderWithState.latencies` is what routing actually uses
- Comment says "Phase 2: integrated into RouterState" but no spec

**Phase 2 specification:**

`LatencyTracker` serves a different role than `ProviderWithState.latencies`:
- `ProviderWithState.latencies`: per-model-group, per-deployment latency tracking
- `LatencyTracker`: **cross-model-group** best provider selection (finds fastest provider across all model groups)

**Integration point:**
```rust
// RouterState holds both
pub struct RouterState {
    pub router: Router,
    pub latency_tracker: LatencyTracker,  // NEW: cross-model-group tracking
}
```

### 2. LatencyTracker.record() Wiring

**Where calls happen:**
- In `Router::record_request_end()` — after provider completes request
- LatencyTracker receives (provider_name, latency_us, optional TTFT) tuple
- Updates samples for that provider regardless of model group

**RouterState integration (see §4 for TTFT details):**
```rust
pub fn record_request_end(
    &mut self,
    model_group: &str,
    index: usize,
    latency_us: u64,
    tokens: u32,
    ttft_us: Option<u64>,  // NEW: optional TTFT for streaming
) {
    // Existing: update ProviderWithState
    self.router.record_request_end(model_group, index, latency_us, tokens);

    // NEW: update cross-model-group tracker with optional TTFT
    if let Some(p) = self.router.get_provider(model_group, index) {
        self.latency_tracker.record(&p.provider.name, latency_us, ttft_us);
    }
}
```

### 3. best_provider_with_ttft() Usage

`LatencyTracker::best_provider_with_ttft()` returns `Option<&str>` (provider name with lowest avg latency across all model groups, using TTFT selection mode for streaming).

**Use case:** When a model group has multiple deployments/providers, `best_provider_with_ttft()` can inform fallback selection even when routing to a different model group.

**NOT used for:** Primary routing (Router.route() uses ProviderWithState.latencies). `best_provider_with_ttft()` is for cross-model-group fallback logic.

### 4. TTFT (Time-To-First-Token) Tracking

TTFT tracking is **specified in RFC-0925** (Accepted). Phase 2 does NOT re-spec TTFT — it wires the RFC-0925 `LatencyTracker` into `RouterState`.

**RFC-0925 TTFT behavior (per §TTFT-Aware Scoring):**
- Streaming with TTFT data: use TTFT only (selection mode, NOT weighted blend)
- Non-streaming or streaming without TTFT: use latency only

**Phase 2 integration:** `record_request_end()` must accept an optional `ttft_us` parameter and pass it to `LatencyTracker::record()` per RFC-0925.

### 5. RouterState::new() Modification

Note: `record_request_end()` signature changes per §2 and §4 — it now accepts optional TTFT parameter.

```rust
impl RouterState {
    pub fn new(config: RouterConfig, providers: Vec<Provider>) -> Self {
        Self {
            router: Router::new(config, providers),
            latency_tracker: LatencyTracker::new(),
        }
    }
}
```

## Key Files to Modify

| File | Change |
|------|--------|
| `crates/quota-router-core/src/router.rs` | Add `RouterState` struct with `router` + `latency_tracker`, wire `record_request_end()` to call both |
| `LatencyTracker` struct | Modify `record()` signature to accept optional TTFT parameter (TTFT spec per RFC-0925) |
| `best_provider_with_ttft()` | TTFT-aware scoring per RFC-0925 §TTFT-Aware Scoring (selection mode, not weighted blend) |

## Acceptance Criteria

- [ ] `RouterState` struct owns both `Router` and `LatencyTracker`
- [ ] `RouterState::record_request_end()` updates both ProviderWithState and LatencyTracker (TTFT flows: request_end → record() → best_provider_with_ttft())
- [ ] `LatencyTracker::record()` accepts optional TTFT parameter (per RFC-0925)
- [ ] `LatencyTracker::best_provider_with_ttft()` uses TTFT selection mode for streaming (per RFC-0925 §TTFT-Aware Scoring)
- [ ] All Phase 3 tests still pass
- [ ] `cargo build -p quota-router-core --features litellm-mode` passes (verify feature gates for latency tracking)
- [ ] `cargo test -p quota-router-core --lib` passes

## Deferred Items (Future Work)

These are explicitly out of scope for Phase 2 but specced in other RFCs:

| Item | Status | RFC |
|------|--------|-----|
| TPM/RPM per-minute bucket tracking | **Specced in RFC-0924** (Accepted) | RFC-0924: Provider Metrics Bucket Tracking |
| Alerting when latency exceeds threshold | **Specced in RFC-0905** (planned) | RFC-0905: Observability and Logging |
| Latency-based routing extensions (TTFT, cooldown) | **Specced in RFC-0925** (Accepted) | RFC-0925: Latency-Based Routing Extensions |
| Autoscaling (infrastructure-level) | **Not applicable** — K8s HPA, not quota-router core | N/A |

### Why TPM/RPM Buckets Need Spec

`ProviderWithState.current_rpm` is a simple counter incremented by `request_ended()`. It does NOT track per-minute buckets like litellm:

```python
# litellm pattern (from lowest_latency.py):
precise_minute = f"{current_date}-{current_hour}-{current_minute}"
# f"{date:hour:minute}" -> "2026-05-11-14-30"
# Stores: { model_group: { deployment_id: { latency: [...], "2026-05-11-14-30": { tpm: 34, rpm: 3 } } } }
```

Our current `current_rpm` cannot answer "what was my RPM at 2:30pm?" — it's a running total, not a time-bucketed histogram.

**If we want latency-aware routing with TPM/RPM enforcement**, we need bucketed tracking. This is separate from LatencyTracker's cross-model-group selection.

## Notes

- Per RFC-0917 A3 Router: routing is non-normative pseudocode — actual implementation may differ while maintaining equivalent behavior
- LatencyTracker uses integer microseconds (u64) per RFC-0104 determinism requirement
- VecDeque with maxlen=100 provides O(1) eviction per provider sample window
- **Pre-implementation verification required:** Check both Phase 3 implementation AND RFC-0925 §TTFT-Aware Scoring to confirm `record()` signature. Phase 3 may have `(provider: &str, latency_us: u64)` while RFC-0925 specifies TTFT support. Phase 2 must extend `record()` to accept optional TTFT parameter per RFC-0925.
- **RFC-0925 spec note:** `best_provider_with_ttft()` declares `ttft_weight: f32` parameter but implementation ignores it (uses selection mode, not weighted blend). This is a dead parameter in RFC-0925 — implementer should verify current status before Phase 2 implementation
