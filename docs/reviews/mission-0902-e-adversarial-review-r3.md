# Adversarial Review Round 3: Mission 0902-e — RFC-0902 v1.5 Alignment

**Reviewer:** Code Review Agent
**Date:** 2026-04-25
**Mission:** `missions/open/0902-e-routing-metrics-alignment.md` (v3, post-R2 fixes)
**RFC:** RFC-0902 v1.5 (Accepted)
**Code:** `crates/quota-router-core/src/router.rs`, `crates/quota-router-core/src/providers.rs`

---

## Executive Summary

| Severity | Count |
|----------|-------|
| CRITICAL | 2 |
| HIGH | 2 |
| MEDIUM | 2 |
| LOW | 1 |

R2 fixes (global weights map, record_success call site, latency_based_impl update) were correctly applied. This round finds implementation-level gaps: `Weighted` strategy cannot be implemented as specified because `simple_shuffle_impl` has no access to `RouterConfig.weights`, the `Weighted` enum variant is missing from the `route()` match, and Display/FromStr are incomplete. Also: stale RFC version in mission header (v1.3 → should be v1.5).

---

## CRITICAL Issues

### CRITICAL-1: `Weighted` Strategy — `simple_shuffle_impl` Has No Access to `config.weights`

**Finding:** The mission specifies `Weighted` implementation:
> "For each provider, look up `config.weights.get(provider.model_name)`. If found, use that weight. If not found, fall back to `get_routing_weight()`"

But `simple_shuffle_impl` is defined as:
```rust
fn simple_shuffle_impl(providers: &[ProviderWithState]) -> usize { ... }
```

It receives ONLY `&[ProviderWithState]` — **no access to `RouterConfig.weights`**.

The `route()` method passes providers to strategy methods:
```rust
let selected_idx = match strategy {
    RoutingStrategy::SimpleShuffle => Self::simple_shuffle_impl(providers),
    ...
};
```

**Problem:** `simple_shuffle_impl` cannot look up `config.weights.get(provider.model_name)` because it doesn't have the config. All existing strategy methods (`simple_shuffle_impl`, `least_busy_impl`, `latency_based_impl`, `usage_based_impl`) take only `&[ProviderWithState]` — they have no knowledge of `RouterConfig`.

**Fix required:** Either:
1. Add a new `weighted_impl(providers: &[ProviderWithState], weights: &HashMap<String, u32>) -> usize` method, OR
2. Refactor `simple_shuffle_impl` to take an additional `weights: &HashMap<String, u32>` parameter

**Recommendation:** Option 1 — new `weighted_impl` method. Keep `simple_shuffle_impl` unchanged for `SimpleShuffle` (uses per-provider `get_routing_weight()`). `Weighted` gets its own method that does global-weights lookup + fallback.

---

### CRITICAL-2: `Weighted` Not in `route()` Match Statement

**Finding:** Even if `weighted_impl` is created, the match statement in `route()` (lines 220-237) does NOT include `RoutingStrategy::Weighted`:

```rust
let selected_idx = match strategy {
    RoutingStrategy::SimpleShuffle => Self::simple_shuffle_impl(providers),
    RoutingStrategy::RoundRobin => { ... }
    RoutingStrategy::LeastBusy => Self::least_busy_impl(providers),
    RoutingStrategy::LatencyBased => Self::latency_based_impl(providers, latency_window),
    RoutingStrategy::CostBased => Self::simple_shuffle_impl(providers),
    RoutingStrategy::UsageBased => Self::usage_based_impl(providers),
    // ❌ Weighted is MISSING
};
```

**Fix required:** Add `RoutingStrategy::Weighted => Self::weighted_impl(providers, &self.config.weights)` to the match.

---

## HIGH Issues

### HIGH-1: Mission Header Says RFC-0902 v1.3 — Stale Version

**Finding:** Mission header:
```
RFC-0902 v1.3 (Accepted): Multi-Provider Routing and Load Balancing
```

But after R2 fixes, RFC is at **v1.5**. The version history table in the mission is also blank (no version history section).

**Impact:** Reviewer checking against v1.3 would miss clarifications added in v1.4/v1.5 (global weights map, Weighted fallback behavior). The mission should reference the current RFC version.

**Fix required:** Update mission header to "RFC-0902 v1.5 (Accepted)". Add version history section documenting v2 changes.

---

### HIGH-2: `Display` and `FromStr` for `Weighted` — Implementation Gap

**Finding:** Acceptance criteria says:
> "`Display` and `FromStr` updated for `Weighted` variant"

Current code (lines 28-54) has:
```rust
impl std::fmt::Display for RoutingStrategy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RoutingStrategy::SimpleShuffle => write!(f, "simple-shuffle"),
            RoutingStrategy::RoundRobin => write!(f, "round-robin"),
            ...
            // ❌ Weighted NOT handled
        }
    }
}
```

`FromStr` similarly doesn't handle `Weighted`. This is correctly listed in acceptance criteria, but it's a concrete implementation gap — not just a documentation note.

**Fix required:** Add `RoutingStrategy::Weighted => write!(f, "weighted")` in both Display and FromStr.

---

## MEDIUM Issues

### MED-1: `Weighted` Global Weights Map — Model Name vs Provider Name Ambiguity

**Finding:** Mission says: "For each provider, look up `config.weights.get(provider.model_name)`"

But `provider.model_name` is the **model group**, not the provider name. In a multi-provider setup:
```rust
Provider { name: "openai", model_name: Some("gpt-3.5-turbo") }
Provider { name: "azure", model_name: Some("gpt-3.5-turbo") }
```

Both have `model_name = "gpt-3.5-turbo"`. If `RouterConfig.weights = {"gpt-3.5-turbo": 10}`, **both** openai and azure get weight 10. The global weights map cannot distinguish between providers within the same model group.

**RFC-0902 YAML example:**
```yaml
weights:
  openai: 10        # ← "openai" looks like provider name, not model_name
  anthropic: 5       # ← "anthropic" looks like provider name
  google: 3
```

But mission says "model_name → weight". These are inconsistent.

**Resolution needed:** Is the weights map keyed by **provider name** (like RFC YAML suggests) or **model_name** (like mission implementation note says)? The mission must pick one and be consistent.

**Recommended fix:** Change mission to say `config.weights.get(provider.name)` — keyed by provider name (name field), not model_name. This allows different weights for different providers sharing the same model.

---

### MED-2: `ProviderWithState` Has No `success_count` / `total_count` Fields

**Finding:** Current code (lines 86-97):
```rust
pub struct ProviderWithState {
    pub provider: Provider,
    pub active_requests: u32,
    pub latencies: Vec<f64>,          // ❌ still f64, needs Vec<u64>
    pub current_rpm: u32,
    pub current_tpm: u32,
    // ❌ success_count and total_count MISSING
}
```

The mission's implementation notes show the correct struct with all fields, but the current code doesn't have them. This isn't a bug in the mission — it's correct. But the mission doesn't explicitly call out that `success_count: u64, total_count: u64` need to be **added** as new fields (not just "updated"). The acceptance criteria says "`ProviderWithState` add `success_count: u64, total_count: u64` fields" which covers it, but the gap is real.

**Status:** Already in acceptance criteria. No fix needed beyond implementation.

---

## LOW Issues

### LOW-1: `Weighted` YAML Weights Interpretation — All Providers in Group Get Same Weight

**Finding:** Even if MED-1 is resolved by using `provider.name` as the key, there's a subtle issue. The `Weighted` strategy selects among providers **within a model group**. The weights map is global across all model groups. If:
```yaml
weights:
  openai: 10
  azure: 5
```

And model_group "gpt-3.5-turbo" has providers [openai, azure], then `Weighted` looks up by provider name and gets [10, 5] — works fine.

But if model_group "claude-3" has providers [anthropic, vertex], and `weights: {anthropic: 5}` only (no vertex entry), then vertex falls back to `get_routing_weight()` which uses its own rpm/tpm config.

**This is actually correct behavior** per the mission's fallback specification. Just noting it works as designed.

---

## Pre-Existing Issues (Not Introduced by Mission)

### KNOWN-1: RFC Uses `ProviderState`, Code Uses `ProviderWithState`

Noted in Round 1. Pre-existing RFC/code naming discrepancy. No action in this mission.

### KNOWN-2: LiteLLM RoutingStrategy Enum Missing `Weighted`

LiteLLM's Python enum does NOT include `Weighted`. This is a custom addition by the RFC. Mission correctly implements RFC spec.

### KNOWN-3: RouterConfig Lives in router.rs, Not config.rs

RFC-0902 Key Files table says:
| `crates/quota-router-core/src/config.rs` | RouterConfig, routing settings |

But `RouterConfig` is actually in `crates/quota-router-core/src/router.rs` (lines 57-83). The `config.rs` file has `Config` (the app-level config with balance, providers, proxy_port). This is a pre-existing discrepancy between RFC and code.

---

## What Was Fixed in R2 ✅

| Issue | Status |
|-------|--------|
| CRITICAL-1 (R2): RouterConfig missing global `weights` HashMap | ✅ Fixed — added to acceptance criteria + RouterConfig design in implementation notes |
| HIGH-1 (R2): `record_success()` not in acceptance criteria | ✅ Fixed — added to acceptance criteria |
| HIGH-2 (R2): `record_success()` call site not specified | ✅ Fixed — documented as external to Router |
| MED-1 (R2): `latency_based_impl` uses `avg_latency()` | ✅ Fixed — added to acceptance criteria + implementation notes |
| MED-2 (R2): Success tracking API not specified | ✅ Fixed — call flow documented |
| MED-3 (R2): Weighted implementation approach | ✅ Fixed — global weights lookup + fallback |

---

## Required Fixes Summary

| Issue | Priority | Fix | Status |
|-------|----------|-----|--------|
| CRITICAL-1: `simple_shuffle_impl` has no access to `config.weights` | MUST FIX | Added `weighted_impl(providers, weights) -> usize` method to implementation notes | ✅ Fixed |
| CRITICAL-2: `Weighted` not in `route()` match | MUST FIX | Added `Weighted` arm to `route()` match in implementation notes | ✅ Fixed |
| HIGH-1: Mission header says RFC-0902 v1.3 (stale) | MUST FIX | Updated to v1.5 | ✅ Fixed |
| HIGH-2: Display/FromStr missing Weighted variant | MUST FIX | Still in acceptance criteria — implementation required | ⏳ Pending |
| MED-1: Weights map keyed by model_name vs provider.name | MUST RESOLVE | Clarified: use `provider.name` (not model_name) for weights lookup | ✅ Fixed |

---

## Version History

| Version | Date | Changes |
|---------|------|---------|
| v1.0 | 2026-04-25 | Initial Round 3 adversarial review — 2 CRITICAL, 2 HIGH, 2 MED, 1 LOW |