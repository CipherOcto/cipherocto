# Adversarial Review Round 2: Mission 0902-e — RFC-0902 v1.3 Alignment

**Reviewer:** Code Review Agent
**Date:** 2026-04-25
**Mission:** `missions/open/0902-e-routing-metrics-alignment.md` (v2, post-R1 fixes)
**RFC:** RFC-0902 v1.4 (Accepted)
**Code:** `crates/quota-router-core/src/router.rs`

---

## Executive Summary

| Severity | Count |
|----------|-------|
| CRITICAL | 1 |
| HIGH | 2 |
| MEDIUM | 3 |
| LOW | 2 |

Round 1 issues (file path, latency storage design, Weighted semantics, typo) were correctly fixed. This round finds deeper implementation gaps.

---

## CRITICAL Issues

### CRITICAL-1: Weighted Strategy — `RouterConfig` Missing Global `weights` Map

**Finding:** The mission says `Weighted` uses "explicit weights from config" distinct from `SimpleShuffle`'s rpm-based weights. But `RouterConfig` (code, lines 57-83) has NO global `weights` map — only per-provider `Provider.weight` field.

**Code analysis:**
```rust
// RouterConfig — NO weights HashMap
pub struct RouterConfig {
    pub routing_strategy: RoutingStrategy,
    pub latency_window: usize,
    pub verbose: bool,
}

// Provider has per-provider weight
pub struct Provider {
    pub weight: Option<u32>,  // per-provider, not global
}

// get_routing_weight() checks per-provider weight FIRST
pub fn get_routing_weight(&self) -> u32 {
    if let Some(w) = self.weight { return w; }  // explicit weight
    if let Some(r) = self.rpm { return r; }
    if let Some(t) = self.tpm { return t / 1000; }
    1
}
```

**Problem:** `Weighted` strategy cannot distinguish itself from `SimpleShuffle` because `get_routing_weight()` already prefers explicit per-provider weights. If a provider has `weight: Some(10)`, both `SimpleShuffle` and `Weighted` would use 10.

**What `Weighted` needs:** A `HashMap<String, u32>` in `RouterConfig` mapping model_name → weight, checked BEFORE per-provider `get_routing_weight()`.

**RFC reference:** RFC-0902 v1.4 YAML example (lines 143-146):
```yaml
weights:
  openai: 10
  anthropic: 5
  google: 3
```

This is a GLOBAL weights map, not per-provider. The code doesn't have this.

**Fix required:** Either:
1. Add `weights: HashMap<String, u32>` to `RouterConfig` and implement `Weighted` to use it, OR
2. Change mission to clarify `Weighted` uses per-provider `weight` field (same as current `get_routing_weight()`) — but then `Weighted` is redundant with `SimpleShuffle`

---

## HIGH Issues

### HIGH-1: `record_success()` Method Missing from `ProviderWithState`

**Finding:** Mission implementation notes (line 76-78) show:
```rust
pub fn record_success(&mut self) {
    self.success_count = self.success_count.saturating_add(1);
}
```

But this method does NOT exist in the current code, and the mission does NOT include it in acceptance criteria.

**Current code:** `ProviderWithState` has no `record_success()` method (lines 99-147).

**Acceptance criteria gap:** The acceptance criteria (lines 23-36) don't list `record_success()` as a required method to add. But the success_count increment logic in implementation notes (lines 100-104) requires it.

**Fix required:** Add `record_success()` to acceptance criteria and implement it.

---

### HIGH-2: `record_success()` Call Site Not Specified

**Finding:** Even if `record_success()` is implemented, where does the Router call it?

**Mission says:** "The router calls `record_success()` after a successful provider response, before `request_ended()`"

**Code analysis:** The Router has `record_request_end()` (lines 312-325):
```rust
pub fn record_request_end(&mut self, model_group: &str, index: usize, latency_ms: f64, tokens: u32) {
    let latency_window = self.config.latency_window;
    if let Some(providers) = self.providers.get_mut(model_group) {
        if let Some(p) = providers.get_mut(index) {
            p.request_ended(latency_ms, tokens, latency_window);
        }
    }
}
```

There is NO `record_success()` call anywhere. The mission needs to specify:
- Does `record_request_end()` get a new `success: bool` parameter?
- Does `record_success()` get called separately by the caller of the router?
- Does a new method like `record_request_end_success()` replace `record_request_end()`?

**Fix required:** Specify the call site and API for success tracking.

---

## MEDIUM Issues

### MED-1: `avg_latency()` Removal vs. RFC-0902 Line 285

**Finding:** Acceptance criteria (line 31) says "`avg_latency()` removed". But RFC-0902's `latency_based_impl` (line 285) references `avg_latency()` in the code.

**Code (lines 279-290):**
```rust
fn latency_based_impl(providers: &[ProviderWithState], _latency_window: usize) -> usize {
    providers
        .iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| {
            a.avg_latency()  // ← this method must exist or rename needed
                .partial_cmp(&b.avg_latency())
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|(i, _)| i)
        .unwrap_or(0)
}
```

If `avg_latency()` is removed and replaced by `avg_latency_us()`, this call site must be updated. The acceptance criteria should explicitly mention updating `latency_based_impl` to call `avg_latency_us()`.

**Fix required:** Add "Update `latency_based_impl` to call `avg_latency_us()` instead of `avg_latency()`" to acceptance criteria.

---

### MED-2: `success` Field Needed on `record_request_end` or New Method

**Finding:** To track success/failure, `record_request_end` needs a `success: bool` parameter, or a separate `record_request_success()` method is needed.

**Current signature (line 312):**
```rust
pub fn record_request_end(&mut self, model_group: &str, index: usize, latency_ms: f64, tokens: u32)
```

**Options:**
1. Add `success: bool` parameter — but this changes the call site API
2. Split into `record_request_success()` and `record_request_end()` — but which gets called when?
3. Call `record_success()` separately before `record_request_end()` — but this requires TWO method calls from router client

**Fix required:** Specify the API design decision.

---

### MED-3: `Weighted` Implementation — `simple_shuffle_impl` or New Method?

**Finding:** The mission doesn't specify whether `Weighted` needs a new implementation method or can reuse `simple_shuffle_impl`.

**Code (lines 220-237):**
```rust
let selected_idx = match strategy {
    RoutingStrategy::SimpleShuffle => Self::simple_shuffle_impl(providers),
    RoutingStrategy::RoundRobin => { ... }
    RoutingStrategy::LeastBusy => Self::least_busy_impl(providers),
    RoutingStrategy::LatencyBased => Self::latency_based_impl(providers, latency_window),
    RoutingStrategy::CostBased => Self::simple_shuffle_impl(providers), // Fallback
    RoutingStrategy::UsageBased => Self::usage_based_impl(providers),
};
```

`Weighted` is not in this match. Adding it requires either:
1. A new `weighted_impl()` method, OR
2. Reuse `simple_shuffle_impl()` but with different weight source

**Fix required:** Specify implementation approach for `Weighted`.

---

## LOW Issues

### LOW-1: `request_ended` Token Parameter — `u32` vs `u64`

**Finding:** The mission shows `request_ended(&mut self, latency_us: u64, tokens: u32, latency_window: usize)`. Token count is `u32` in current code.

**RFC-0902 v1.4 lines 206-209:**
```rust
/// Success and total counts (integer). Ratio computed at display time only —
success_count: u64,
total_count: u64,
```

Token counts (`u32`) are separate from success counts (`u64`). The mission is correct that tokens are still `u32`. No issue — just confirming.

---

### LOW-2: `Router::route()` — No Success Tracking Integration

**Finding:** `Router::route()` (lines 208-238) doesn't track success. It calls `route()` and `record_request_start/end()`. Success tracking is not integrated into the routing flow.

**Question:** Does success tracking happen inside `route()` (after provider response) or outside (caller tracks success/failure)?

**Current flow:** Router doesn't know if a request succeeded — it only tracks latency. Success tracking requires the caller to inform the router after the provider response.

**Fix required:** Document that success tracking is external to Router, and `record_success()` is called by the router client after a successful response.

---

## Pre-Existing Issues (Not Introduced by Mission)

### KNOWN-1: RFC Uses `ProviderState`, Code Uses `ProviderWithState`

Noted in Round 1. This is a pre-existing RFC/code discrepancy, not a mission bug.

### KNOWN-2: LiteLLM RoutingStrategy Enum Missing `Weighted`

LiteLLM's Python enum (RFC-0902 lines 117-124) does NOT include `Weighted`. This is a custom addition by the RFC. The mission correctly implements what the RFC specifies.

---

## What Was Fixed in Round 1 ✅

| Issue | Status |
|-------|--------|
| File path wrong | ✅ Fixed — quota-router-core |
| Latency storage loses sliding window | ✅ Fixed — Vec<u64> samples |
| Weighted semantics ambiguous | ✅ Clarified — explicit config vs rpm-derived |
| Typo ProviderBudgetLimancing | ✅ Fixed |
| Display/FromStr missing | ✅ In acceptance criteria |

---

## Required Fixes Summary

| Issue | Priority | Fix | Status |
|-------|----------|-----|--------|
| CRITICAL-1: Weighted needs global `weights` HashMap | MUST RESOLVE | Add `weights: HashMap<String, u32>` to RouterConfig | ✅ Fixed |
| HIGH-1: `record_success()` method not in acceptance criteria | MUST ADD | Added to acceptance criteria | ✅ Fixed |
| HIGH-2: `record_success()` call site not specified | MUST SPECIFY | Documented as external to Router (router client calls it) | ✅ Fixed |
| MED-1: `latency_based_impl` uses `avg_latency()` | MUST ADD | Added to acceptance criteria + implementation notes | ✅ Fixed |
| MED-2: API for success tracking not specified | MUST SPECIFY | Documented call flow for success/failure cases | ✅ Fixed |
| MED-3: Weighted implementation approach | MUST SPECIFY | Specified: global weights lookup + fallback to get_routing_weight() | ✅ Fixed |

---

## Version History

| Version | Date | Changes |
|---------|------|---------|
| v1.0 | 2026-04-25 | Initial Round 2 adversarial review |
| v1.1 | 2026-04-25 | Add LOW-2 (Router route() no success tracking integration) |