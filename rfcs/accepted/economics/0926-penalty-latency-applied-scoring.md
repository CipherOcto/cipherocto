# RFC-0926 (Economics): Penalty Latency Applied to Scoring

## Status

Accepted (v12 — 2026-05-12)

## Authors

- Author: @cipherocto

## Maintainers

- Maintainer: @cipherocto

## Summary

Integrate penalty latencies stored in `CooldownTracker` into `LatencyTracker` scoring for latency-based routing decisions. When a deployment experiences timeouts or failures, the penalty latency (from `LatencyConfig.timeout_penalty_us`, default 1_000_000_000µs per litellm) should be factored into provider selection to avoid routing to degraded deployments.

## Why Needed

RFC-0925 introduced `CooldownTracker` with `penalty_latencies` field to track timeout/failure penalties. However, these penalties are stored but never applied to scoring:

- **Current behavior:** `penalty_latencies` is appended via `record_timeout_penalty()` but never read
- **Desired behavior:** Penalty latencies should affect provider selection via the new `best_provider_with_penalties()` helper
- **Impact:** A deployment that timed out once gets a 1000-second penalty added to its latency average, correctly routing traffic away until the cooldown expires

## Dependencies

**Requires:**
- RFC-0925: Latency-Based Routing Extensions (CooldownTracker, penalty_latencies field)
- RFC-0917: Dual-Mode Query Router (LatencyTracker)

## Design Decisions

### Single Source of Truth

`CooldownTracker.penalty_latencies` is the **single source of truth**. `LatencyTracker` does NOT store its own penalty latencies — it queries `CooldownTracker` when computing effective latency.

**Why:** Avoids synchronization risk between two stores. When `record_timeout_penalty()` is called, only one store is updated.

### Penalty Expiry

Penalty latencies expire when the cooldown expires (per `is_cooldown_expired()`). When `update_latency_state()` detects cooldown expiry and transitions to `Healthy` state, it calls `clear_penalty_latencies()`.

### TTFT Scoring

Penalty latencies affect **regular latency scoring only**, NOT TTFT scoring. Rationale:
- TTFT (time-to-first-token) measures initial responsiveness
- A timeout that occurs AFTER first token shouldn't penalize TTFT
- Only the total latency sample should include the penalty

## Scope

### Data Structures

No new struct fields needed. `CooldownTracker.penalty_latencies: Vec<u64>` already exists. `LatencyTracker` does NOT store penalty latencies — it receives them via `best_provider_with_penalties()` parameter.

```rust
// CooldownTracker already has:
// penalty_latencies: Vec<u64>

// LatencyTracker does NOT get a penalty_latencies field
// It receives penalty_map parameter in best_provider_with_penalties()

// Router integration builds local HashMap<String, Vec<u64>> at selection time
```

### New/Modified Methods

| Method | Signature | Notes |
|--------|-----------|-------|
| `get_penalty_latencies` | `pub fn get_penalty_latencies(&self) -> &[u64]` | Returns reference to penalty latencies for query (on CooldownTracker, defined in RFC-0925, used by RFC-0926) |
| `best_provider_with_penalties` | `pub fn best_provider_with_penalties(&self, penalty_map: &HashMap<String, Vec<u64>>, available_names: &HashSet<&str>, is_streaming: bool) -> Option<(&str, f32)>` | Returns (name, effective_latency) considering penalty-adjusted scores; only considers providers in `available_names` set; TTFT-only for streaming (new helper on LatencyTracker) |

**Note:** `record_timeout_penalty`, `clear_penalty_latencies`, and `get_penalty_latencies` are defined in RFC-0925. RFC-0926 depends on them but does not re-spec them.

### Scoring Integration

When computing provider scores:

```
effective_latency = (sum(samples) + sum(penalty_latencies)) / (len(samples) + len(penalty_latencies))
```

For TTFT scoring specifically, penalty latencies are NOT included:
```
ttft_score = avg_ttft_samples  // No penalty adjustment
```

**Implementation note:** `best_provider_with_penalties()` is a new helper method on `LatencyTracker` that accepts a `penalty_map` and `available_names` parameter. The `available_names` set filters out cooldown providers before scoring. The existing `best_provider()` and `best_provider_with_ttft()` methods are unchanged — penalty-adjusted selection uses the new helper.

**Signature verification required:** The `best_provider_among(available_names, is_streaming)` call in the Router integration (Updated integration in Router section) must match the actual signature from RFC-0917 implementation. Verify: `pub fn best_provider_among(&self, available: HashSet<&str>, is_streaming: bool) -> Option<&str>`.

**Note on `lowest_latency_buffer`:** Unlike `best_provider_with_ttft()`, this method does not apply `lowest_latency_buffer` filtering. The penalty mechanism already serves as a strong discriminator — penalized providers have effective latencies ~100x worse, naturally routing traffic away without explicit buffer filtering. When no penalties exist, the caller should use the existing buffer-aware selection path.

### Integration Flow

```
1. Request completion handler detects failure
   → Timeout or retryable failure: HTTP 408, 409, 429, or 5xx
     (Note: 401/404 are cooldown-eligible via failure rate but NOT retryable per litellm _should_retry())
   → Calls `cooldown_tracker.record_timeout_penalty(config.timeout_penalty_us)`
   → penalty_latencies.push(config.timeout_penalty_us)
   → Penalty value comes from LatencyConfig.timeout_penalty_us (default: 1_000_000_000µs per litellm)
   → (Non-retryable errors call record_error() instead)

2. Request completion handler calls update_latency_state()
   → Records success/failure in CooldownTracker
   → On cooldown expiry: CooldownTracker.clear_penalty_latencies()

3. Router.route() calls latency_based_with_cooldown_impl() for provider selection
   → First filters out providers in cooldown (via is_available()) into available_names set
   → Builds penalty_map from CooldownTrackers of available providers
   → Calls latency_tracker.best_provider_with_penalties(penalty_map, available_names, is_streaming)
     OR best_provider_among(available_names, is_streaming) if no penalties
   → Returns Some(index) of best provider, or None if no valid providers
   → For streaming with TTFT data: penalties NOT applied (TTFT-only)
   → For streaming without TTFT or non-streaming: effective_latency includes penalties

4. On None return from latency_based_with_cooldown_impl():
   → All available providers have no latency samples (fresh deployment)
   → The **caller** of `latency_based_with_cooldown_impl()` (e.g., `Router.route()`) should fall back to a non-latency-based strategy (e.g., round-robin, first-available)
   → Or return RouterError::NoAvailableProviders (from RFC-0920 error types)
```

### CooldownTracker Changes

`clear_penalty_latencies()` is defined in RFC-0925. RFC-0926 adds:

```rust
impl CooldownTracker {
    /// Get reference to penalty latencies for external query
    pub fn get_penalty_latencies(&self) -> &[u64] {
        &self.penalty_latencies
    }
}
```

### Scoring Integration (Implementation Detail)

The penalty-adjusted scoring happens in `Router::latency_based_with_cooldown_impl()`. The method:

1. Filters providers to only available (non-cooldown) ones into `available_names`
2. Builds `penalty_map` from `CooldownTracker.get_penalty_latencies()` for available providers
3. Calls `best_provider_with_penalties(penalty_map, available_names, is_streaming)` which only scores providers in `available_names`

**Proposed helper method on LatencyTracker:**

```rust
impl LatencyTracker {
    /// Get best provider considering penalty-adjusted latencies
    /// Returns (provider_name, effective_latency) or None if no valid providers.
    ///
    /// NOTE: Only considers providers with latency samples in self.samples AND
    /// that are present in the `available_names` set. Callers must populate
    /// `available_names` with providers that are not in cooldown.
    /// Fresh deployments (no latency samples) cannot be selected by this method;
    /// callers should use a separate strategy when this returns None.
    pub fn best_provider_with_penalties(
        &self,
        penalty_map: &std::collections::HashMap<String, Vec<u64>>,  // provider_name -> penalties
        available_names: &std::collections::HashSet<&str>,  // only consider providers in this set
        is_streaming: bool,
    ) -> Option<(&str, f32)> {
        let mut candidates: Vec<(&str, f32)> = Vec::new();

        for (name, samples) in &self.samples {
            let name_str = name.as_str();

            // Filter: must be in available set (not in cooldown) and have samples
            if !available_names.contains(name_str) {
                continue;
            }
            if samples.is_empty() {
                continue;
            }

            // For streaming with TTFT data: use TTFT only, ignore penalties
            // This is the design decision: TTFT measures initial responsiveness,
            // a timeout AFTER first token shouldn't penalize TTFT
            if is_streaming {
                if let Some(ttft_samples) = self.ttft_samples.get(name_str) {
                    if !ttft_samples.is_empty() {
                        let ttft_avg = ttft_samples.iter().sum::<u64>() as f32 / ttft_samples.len() as f32;
                        candidates.push((name_str, ttft_avg));
                        continue;
                    }
                }
            }

            // Non-streaming OR streaming without TTFT data: use penalty-adjusted latency
            let samples_sum: u64 = samples.iter().sum();
            let base_latency = samples_sum as f32 / samples.len() as f32;
            let penalties = penalty_map.get(name_str).map(|p| p.as_slice()).unwrap_or(&[]);

            let effective = if penalties.is_empty() {
                base_latency
            } else {
                let penalty_sum: u64 = penalties.iter().sum();
                let total_count = samples.len() + penalties.len();
                (samples_sum as f32 + penalty_sum as f32) / total_count as f32
            };

            candidates.push((name_str, effective));
        }

        // Find minimum by score using total ordering on f32 bits.
        // For positive finite floats (all realistic latencies), bit ordering matches numeric ordering:
        //   100ms (0x42C80000) < 200ms (0x43480000) < ... < 10s (0x461C4000)
        // Using to_bits() avoids f32::total_cmp (unstable) and is deterministic.
        candidates.iter()
            .map(|(name, score)| (*name, score, score.to_bits()))
            .min_by_key(|(_, _, bits)| *bits)
            .map(|(name, score, _)| (name, *score))
    }
}
```

**Updated integration in Router:**

```rust
use std::collections::HashMap;

fn latency_based_with_cooldown_impl(
    providers: &mut [ProviderWithState],
    latency_tracker: &mut LatencyTracker,
    latency_config: &LatencyConfig,
    is_streaming: bool,
) -> Option<usize> {
    // ... existing cooldown expiry logic ...

    // Build available set (providers not in cooldown) and penalty map
    let mut penalty_map: HashMap<String, Vec<u64>> = HashMap::new();
    let mut available_names: HashSet<&str> = HashSet::new();

    for provider in providers.iter() {
        // Skip providers in cooldown
        if !provider.cooldown_tracker.is_available() {
            continue;
        }

        // NOTE: available_names stores &str references into provider.provider.name (String).
        // The HashSet and HashMap must not outlive the providers slice.
        // Since we iterate and consume within this function, references remain valid.
        let name_str: &str = provider.provider.name.as_str();
        available_names.insert(name_str);

        // Build penalty map for this provider (only if penalties exist)
        let penalties = provider.cooldown_tracker.get_penalty_latencies();
        if !penalties.is_empty() {
            penalty_map.insert(provider.provider.name.clone(), penalties.to_vec());
        }
    }

    // If no available providers, return None
    if available_names.is_empty() {
        return None;
    }

    // Use penalty-adjusted selection when penalties exist, otherwise use standard best_provider_among
    let best_name = if penalty_map.is_empty() {
        // No penalties: use standard selection among available providers
        latency_tracker.best_provider_among(available_names, is_streaming)?
    } else {
        // Penalties exist: use penalty-adjusted selection among available providers
        match latency_tracker.best_provider_with_penalties(&penalty_map, &available_names, is_streaming)? {
            (name, _) => name,
        }
    };

    // Return index of best provider
    // Invariant: best_name must be in providers (from self.samples keys which are provider names)
    // If invariant is violated (data inconsistency), this returns None
    providers.iter()
        .position(|p| p.provider.name.as_str() == best_name)
}
```

### Worked Example

Provider with 99 samples averaging 100ms (100,000μs) + 1 penalty of 1,000,000,000µs (1000 seconds, from `LatencyConfig.timeout_penalty_us`):
- Sum of samples: 99 × 100,000μs = 9,900,000μs
- Sum of penalties: 1 × 1,000,000,000μs = 1,000,000,000μs
- Combined: 1,009,900,000μs
- Effective latency: 1,009,900,000μs / 100 = 10,099,000μs ≈ **10.1 seconds**
- Normal latency without penalty: 100ms
- **Penalty increases effective latency by ~100x**
- Correctly routes traffic away from degraded deployment

## Implementation Prerequisites

RFC-0926 depends on the following existing implementations from RFC-0925:

1. **LatencyConfig.timeout_penalty_us field** — must exist in the config struct
2. **CooldownTracker.record_timeout_penalty()** — must be defined and callable
3. **CooldownTracker.get_penalty_latencies()** — must return reference to penalties
4. **CooldownTracker.clear_penalty_latencies()** — must exist and clear penalties on cooldown expiry
5. **CooldownTracker.is_available()** — must return false when provider is in cooldown (state == Cooldown && !is_cooldown_expired)

**Calling path for record_timeout_penalty():** The caller (e.g., `update_latency_state()` or completion handler) must invoke `record_timeout_penalty(config.timeout_penalty_us)` when detecting timeout/failure. The penalty value must come from `LatencyConfig.timeout_penalty_us`, not hardcoded.

## Open Questions (RESOLVED)

**Q: Should penalty latencies expire on a separate TTL or persist with cooldown duration?**

**A:** They expire WITH the cooldown. When `is_cooldown_expired()` returns true and we transition to `Healthy`, we call `clear_penalty_latencies()`.

**Q: Should penalty latencies affect TTFT scoring?**

**A:** NO. Penalties affect regular latency scoring only. TTFT scoring uses only `ttft_samples` average.

**Q: Should `best_provider_with_penalties` filter by `available_names` internally or should the caller filter before calling?**

**A:** The helper filters internally. The caller passes `available_names` and the helper skips any provider not in that set. This keeps the helper self-contained and reusable for any availability filter, not just cooldown.

**Q: What happens when all available providers have no latency samples?**

**A:** `best_provider_with_penalties` returns `None`. The caller (`latency_based_with_cooldown_impl`) propagates `None` to `Router.route()`. The Router should fall back to a non-latency-based strategy (round-robin, first-available) or return `RouterError::NoAvailableProviders`.

**Q: Should cooldown expiry be detected in `latency_based_with_cooldown_impl` or in `update_latency_state`?**

**A:** Both. `update_latency_state` clears penalties when cooldown expires (step 2). Additionally, `latency_based_with_cooldown_impl` filters by `is_available()` on each call (step 3), so expired cooldowns are implicitly handled by the availability filter.

**Q: Why two branches (best_provider_among vs best_provider_with_penalties) when both filter by available_names?**

**A:** The branches represent different scoring modes: `best_provider_among` uses raw latency samples, while `best_provider_with_penalties` uses penalty-adjusted effective latency. If no penalties exist (`penalty_map.is_empty()`), the adjusted score equals the raw score, but `best_provider_among` is simpler and faster. When penalties exist, `best_provider_with_penalties` must be used to apply the adjustment.

## Future Work

- **Adaptive penalties:** Consider different penalty values per failure type (timeout vs 429 vs error). Would require `record_timeout_penalty(config, FailureType)` with typed penalties instead of uniform `timeout_penalty_us`. Separate design work needed.

## Related RFCs

- RFC-0917: Dual-Mode Query Router (LatencyTracker)
- RFC-0925: Latency-Based Routing Extensions (CooldownTracker, penalty_latencies)

## Version History

| Version | Date | Changes |
|---------|------|---------|
| 12 | 2026-05-12 | Fix per 11th adversarial review: replace stale line reference with section name reference ("Updated integration in Router section"), add use HashSet alongside HashMap for consistent style |
| 11 | 2026-05-12 | Fix per 10th adversarial review: remove orphan code fence between impl blocks, fix redundant samples.sum() by computing once and reusing in penalty branch |
| 10 | 2026-05-12 | Fix per 9th adversarial review: remove use statement from inside impl block (use fully qualified paths instead), fix stale line reference in signature verification note (261→264) |
| 9 | 2026-05-12 | Fix per 8th adversarial review: fix uninitialized best_name in penalty branch (use match instead of expect), update status header v7→v8, clarify to_bits() comment with concrete examples |
| 8 | 2026-05-12 | Fix per 7th adversarial review: remove unused _latency_window parameter, fix min_by_key to use to_bits() for safe float comparison, add signature verification note for best_provider_among, add RouterError::NoAvailableProviders reference, fix variable shadowing (best_name → penalty_best), avoid clone when penalties empty |
| 7 | 2026-05-12 | Fix per 6th adversarial review: specify calling sites (request completion handler, Router.route()), specify None fallback behavior, add 4 resolved Q&A to Open Questions, remove penalty decay from Future Work (conflicts with binary expiry), add is_available() to prerequisites, fix variable shadowing |
| 6 | 2026-05-12 | Fix per 5th adversarial review: add available_names filter to best_provider_with_penalties (was built but never used), filter cooldown providers BEFORE scoring, consistent availability filtering in both penalty and no-penalty paths |
| 5 | 2026-05-12 | Fix per 4th adversarial review: update Why Needed to reference new helper not best_provider(), fix fallback to use best_provider_among, clarify clear_penalty_latencies is from RFC-0925, specify timeout/failure types (408/409/429/5xx), document fresh-deployment limitation, update Future Work |
| 4 | 2026-05-12 | Fix per 3rd adversarial review: fix HashMap lifetime (use owned String keys), clarify TTFT logic (penalties excluded for streaming+TTFT only), return Option<usize> from Router integration, update Integration Flow, add score comment |
| 3 | 2026-05-12 | Fix per 2nd adversarial review: replace non-existent get_effective_latency with best_provider_with_penalties helper, fix worked example math, add scoring integration implementation detail |
| 2 | 2026-05-12 | Fix per adversarial review: single source of truth, penalty expiry clarified, TTFT question resolved |
| 1 | 2026-05-12 | Initial planned RFC |
