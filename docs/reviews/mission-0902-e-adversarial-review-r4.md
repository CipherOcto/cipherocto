# Adversarial Review Round 4: Mission 0902-e — RFC-0902 v1.5 Alignment

**Reviewer:** Code Review Agent
**Date:** 2026-04-25
**Mission:** `missions/open/0902-e-routing-metrics-alignment.md` (v4, post-R3 fixes)
**RFC:** RFC-0902 v1.5 (Accepted)
**Code:** `crates/quota-router-core/src/router.rs`

---

## Executive Summary

| Severity | Count |
|----------|-------|
| CRITICAL | 2 |
| HIGH | 2 |
| MEDIUM | 2 |
| LOW | 1 |

R3 fixes (weighted_impl method, Weighted in route() match, RFC version update) were correctly applied. This round finds test-level issues: existing tests use f64 latency literals and old record_request_end signature that will break when mission is implemented. Also: failure call flow contradicts implementation (total_count always incremented vs. not incremented on failure).

---

## CRITICAL Issues

### CRITICAL-1: Tests Use f64 Latency Literals — Will Break When `latencies: Vec<u64>`

**Finding:** Existing tests use f64 latency values throughout:

Line 472-474 (`test_latency_based_routing`):
```rust
p.latencies = vec![100.0, 110.0, 105.0]; // Fast: ~105ms avg
p.latencies = vec![500.0, 510.0, 505.0]; // Slow: ~505ms avg
```

Line 531 (`test_request_tracking`):
```rust
router.record_request_end("gpt-3.5-turbo", idx, 150.0, 100);
```

After mission is implemented:
- `latencies` becomes `Vec<u64>` — these f64 literals won't compile
- `record_request_end(latency_ms: f64, ...)` becomes `record_request_end(latency_us: u64, ...)` — `150.0` won't compile

**Impact:** The acceptance criteria says "`cargo test --lib` passes" — but these tests will fail to compile after the mission's changes are applied. They need explicit updates in the mission.

**Fix required:** Add acceptance criteria item: "Test vectors updated: all `vec![f64]` latency values → `vec![u64]` microseconds; `record_request_end(..., f64, ...)` → `record_request_end(..., u64, ...)`"

---

### CRITICAL-2: test_routing_strategy_from_str Missing `Weighted` Test

**Finding:** `test_routing_strategy_from_str` (lines 434-456) tests all strategies' `FromStr` round-trip EXCEPT `Weighted`:

```rust
#[test]
fn test_routing_strategy_from_str() {
    assert_eq!("simple-shuffle".parse::<RoutingStrategy>().unwrap(), RoutingStrategy::SimpleShuffle);
    assert_eq!("round-robin".parse::<RoutingStrategy>().unwrap(), RoutingStrategy::RoundRobin);
    assert_eq!("least-busy".parse::<RoutingStrategy>().unwrap(), RoutingStrategy::LeastBusy);
    assert_eq!("latency-based".parse::<RoutingStrategy>().unwrap(), RoutingStrategy::LatencyBased);
    assert_eq!("usage-based".parse::<RoutingStrategy>().unwrap(), RoutingStrategy::UsageBased);
    // ❌ Missing: "weighted".parse() test
}
```

The acceptance criteria says "`Display` and `FromStr` updated for `Weighted` variant" — but there's no test verifying this. Without a test, there's no verification the implementation is correct.

**Fix required:** Add test: `assert_eq!("weighted".parse::<RoutingStrategy>().unwrap(), RoutingStrategy::Weighted);`

---

## HIGH Issues

### HIGH-1: `RouterConfig::default()` Doesn't Initialize `weights: HashMap`

**Finding:** When the mission adds `weights: HashMap<String, u32>` to `RouterConfig`, the `Default` impl needs to initialize it:

Current `Default` impl (lines 75-83):
```rust
impl Default for RouterConfig {
    fn default() -> Self {
        Self {
            routing_strategy: RoutingStrategy::SimpleShuffle,
            latency_window: 10,
            verbose: false,
            // ❌ Missing when weights field is added
        }
    }
}
```

**Problem:** If `RouterConfig` has a `weights: HashMap<String, u32>` field and `Default` doesn't initialize it, the struct is uninitialized. In Rust, `HashMap` has a default via `Default` trait — but if the field is added without updating the `Default` impl, the code won't compile.

**Fix required:** Add acceptance criteria: "`Default` impl for `RouterConfig` updated to initialize `weights: HashMap::new()`"

---

### HIGH-2: Test Uses Struct Update Syntax — Fragile if Fields Added

**Finding:** `test_round_robin` (line 391-408) uses:
```rust
let config = RouterConfig {
    routing_strategy: RoutingStrategy::RoundRobin,
    ..Default::default()
};
```

This relies on `..Default::default()` filling in remaining fields. If `weights: HashMap<String, u32>` is added to `RouterConfig` and `Default` is properly updated, this works. But the pattern is fragile — adding new required fields without updating `Default` would break this.

**Status:** Works if HIGH-1 is fixed (Default includes `weights: HashMap::new()`). Just noting the pattern is fragile.

---

## MEDIUM Issues

### MED-1: Failure Call Flow Contradicts Implementation (total_count)

**Finding:** Two contradicting statements in the mission:

**Implementation note (lines 77-79):**
```rust
pub fn request_ended(&mut self, latency_us: u64, tokens: u32, latency_window: usize) {
    ...
    self.total_count = self.total_count.saturating_add(1); // ← ALWAYS incremented
}
```

**Failure call flow (lines 147-151):**
```rust
// Failure case:
router.client handles error — no record_success() call
// Failure is tracked separately; total_count is NOT incremented for failures
```

**Analysis:** If `total_count` is incremented inside `request_ended()` (always), then the only way for `total_count` NOT to be incremented on failure is for the caller to NOT call `request_ended()` at all on failure. But then latency isn't tracked either. The success call flow DOES call `record_request_end()` which calls `request_ended()`.

**Question:** Should failures call `record_request_end()` (tracking latency but not success) or skip it entirely?

The current implementation says `total_count` is incremented in `request_ended`. So if you call `record_request_end()` on failure, `total_count` IS incremented (even though `success_count` is not).

The failure call flow says "total_count is NOT incremented for failures" — but the implementation always increments it inside `request_ended`. You can't have it both ways.

**Fix required:** Clarify failure call flow. Two options:
1. On failure: call `record_request_end()` but NOT `record_success()` → `total_count++` but `success_count` unchanged (failure tracked in total, not in success)
2. On failure: skip both `record_success()` and `request_ended()` → nothing tracked for failures

**Recommendation:** Option 1 — failures SHOULD call `record_request_end()` to track latency even when the request fails. This gives visibility into failure latencies (e.g., timeout latencies). Update failure call flow to reflect this.

---

### MED-2: No Tests for `Weighted` Strategy (Pre-Implementation Gap)

**Finding:** No tests exist for the `Weighted` routing strategy. This is expected since the strategy doesn't exist yet — but when it's implemented, it needs tests.

**Test needed:**
```rust
#[test]
fn test_weighted_routing() {
    let providers = test_providers_with_weights();
    let mut config = RouterConfig {
        routing_strategy: RoutingStrategy::Weighted,
        weights: HashMap::from([
            ("openai".to_string(), 10),
            ("azure".to_string(), 1),
        ]),
        ..Default::default()
    };
    let mut router = Router::new(config, providers);

    // Verify weighted selection favors openai (10x azure)
    ...
}
```

**Status:** Not a bug — just a missing future test. The mission should note this in implementation notes.

---

## LOW Issues

### LOW-1: No Tests for `success_count` / `total_count`

**Finding:** None of the existing tests verify `success_count` or `total_count` behavior. After the mission is implemented:
- No test calls `record_success()`
- No test verifies `success_count` increments
- No test verifies `total_count` increments on `request_ended`

**Status:** Pre-implementation gap. These tests need to be added when the mission is implemented. Not a bug in the mission, but the mission's acceptance criteria should explicitly mention adding tests for the new metrics.

---

## Pre-Existing Issues (Not Introduced by Mission)

### KNOWN-1: RFC Uses `ProviderState`, Code Uses `ProviderWithState`

Pre-existing naming discrepancy between RFC pseudocode and actual code. No action in this mission.

### KNOWN-2: LiteLLM RoutingStrategy Enum Missing `Weighted`

LiteLLM doesn't have `Weighted`. RFC custom addition. Mission correctly implements RFC spec.

---

## What Was Fixed in R3 ✅

| Issue | Status |
|-------|--------|
| CRITICAL-1 (R3): `simple_shuffle_impl` has no access to `config.weights` | ✅ Fixed — `weighted_impl` method added to implementation notes |
| CRITICAL-2 (R3): `Weighted` not in `route()` match | ✅ Fixed — `Weighted` arm added to match |
| HIGH-1 (R3): Mission header says RFC-0902 v1.3 (stale) | ✅ Fixed — updated to v1.5 |
| MED-1 (R3): Weights map keyed by provider.name not model_name | ✅ Fixed — clarified |

---

## Required Fixes Summary

| Issue | Priority | Fix | Status |
|-------|----------|-----|--------|
| CRITICAL-1: Tests use f64 latency literals | MUST FIX | Added AC for test vector updates (Vec<u64>, u64 latency_us) | ✅ Fixed |
| CRITICAL-2: test_routing_strategy_from_str missing Weighted | MUST FIX | Added test for "weighted".parse() to acceptance criteria | ✅ Fixed |
| HIGH-1: RouterConfig Default missing weights init | MUST FIX | Added AC: "Default for RouterConfig initializes weights: HashMap::new()" | ✅ Fixed |
| MED-1: Failure call flow contradicts implementation | MUST FIX | Clarified: failures DO call request_ended (total_count++); success_count unchanged | ✅ Fixed |
| MED-2: No tests for Weighted strategy | NOTE | Added test note to implementation notes | ✅ Fixed |

---

## Version History

| Version | Date | Changes |
|---------|------|---------|
| v1.0 | 2026-04-25 | Initial Round 4 adversarial review — 2 CRITICAL, 2 HIGH, 2 MED, 1 LOW |