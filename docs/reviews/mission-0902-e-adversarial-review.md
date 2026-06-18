# Adversarial Review: Mission 0902-e — RFC-0902 v1.3 Alignment

**Reviewer:** Code Review Agent
**Date:** 2026-04-25
**Mission:** `missions/open/0902-e-routing-metrics-alignment.md`
**RFC:** RFC-0902 v1.3 (Accepted)
**Code:** `crates/quota-router-core/src/router.rs`

---

## Executive Summary

| Severity | Count |
|----------|-------|
| CRITICAL | 2 |
| HIGH | 3 |
| MEDIUM | 3 |
| LOW | 2 |

---

## CRITICAL Issues

### CRITICAL-1: File Path Wrong

**Mission says:** `crates/quota-router-cli/src/router.rs`
**Code lives at:** `crates/quota-router-core/src/router.rs`
**RFC says:** `crates/quota-router-cli/src/router.rs`

Three-way mismatch. The mission uses the RFC's stated path, but the code actually lives in `quota-router-core`. The mission MUST be updated with the correct path.

**Fix:** Change mission file path to `crates/quota-router-core/src/router.rs`.

---

### CRITICAL-2: Latency Storage Design — Loss of Per-Sample Data

**Problem:** The mission proposes replacing `latencies: Vec<f64>` (per-sample storage) with `avg_latency_us: u64` (single aggregate). This loses the ability to compute rolling window averages correctly.

**Why this matters:**
- RFC-0902 v1.3 line 155: `latency_window: 100  # Track last N requests`
- The `latency_window` parameter means "track last N latency samples"
- Storing only `avg_latency_us` means you cannot:
  1. Add a new sample and evict the oldest (sliding window)
  2. Recompute the average when the window changes
  3. Know which samples are in the current window vs. expired

**Current code (correct for sliding window):**
```rust
pub latencies: Vec<f64>,           // Stores individual samples
pub fn request_ended(&mut self, latency_ms: f64, latency_window: usize) {
    self.latencies.push(latency_ms);
    if self.latencies.len() > latency_window {
        self.latencies.drain(0..self.latencies.len() - latency_window);
    }
}
```

**Mission proposal (loses sliding window):**
```rust
pub avg_latency_us: u64,           // Only stores aggregate
```

**Required fix:** Keep `latencies: Vec<u64>` (microseconds, not aggregate) and add `avg_latency_us: u64` as a cached computed field. OR compute `avg_latency_us` on-demand from the sample vector without storing it.

---

## HIGH Issues

### HIGH-1: `Weighted` Strategy — Duplicate of `SimpleShuffle`?

**Finding:** The RFC defines 7 strategies including `Weighted` (line 96-97). But examining the definitions:
- `SimpleShuffle`: "Weighted distribution based on rpm/tpm weights" (line 74-75)
- `Weighted`: "Weighted distribution based on configured weights" (line 96-97)

These are functionally identical. The only difference is `SimpleShuffle` uses rpm/tpm weights while `Weighted` uses explicit `weight` configuration. But looking at the code (lines 244-245):
```rust
let weights: Vec<u32> = providers.iter().map(|p| p.get_routing_weight()).collect();
```
`get_routing_weight()` returns the provider's configured weight — which is already what `Weighted` would do.

**Question:** Is `Weighted` actually distinct from `SimpleShuffle`, or is it a duplicate that shouldn't exist?

**RFC-0902 Reference:** The LiteLLM RoutingStrategy enum (lines 117-124) does NOT include `Weighted`. This suggests `Weighted` was added by the RFC author as a separate concept but may not be LiteLLM-compatible.

**Impact:** If `Weighted` is truly distinct, the code needs it. If it's a duplicate, adding it creates redundant code.

**Required resolution:** Clarify whether `Weighted` is semantically distinct from `SimpleShuffle` before implementing.

---

### HIGH-2: Missing `success_count`/`total_count` Increment Logic

**Finding:** The mission says to add `success_count: u64, total_count: u64` but doesn't specify WHERE these are incremented.

**RFC-0902 v1.3 lines 206-209:**
```rust
/// Success and total counts (integer). Ratio computed at display time only —
/// never used for routing decisions (avoids f64 non-determinism per RFC-0104).
success_count: u64,
total_count: u64,
```

**Question:** When are these incremented?
- On every `request_ended` call?
- Only on successful requests (not errors)?
- What counts as "success" — HTTP 2xx only, or also valid AI response?

The RFC doesn't specify. The mission needs to define the increment logic.

**Current code:** No such tracking exists. `request_ended` (lines 116-126) only updates `active_requests`, `latencies`, `current_rpm`, `current_tpm`.

---

### HIGH-3: `request_ended` Signature — Microseconds vs. Milliseconds

**Finding:** If latency is stored in microseconds (`u64`), what is the unit of the `latency_ms: f64` parameter currently in `request_ended`?

**Current signature (line 116):**
```rust
pub fn request_ended(&mut self, latency_ms: f64, tokens: u32, latency_window: usize)
```

The parameter name says `ms` (milliseconds). But if we change to microseconds, either:
1. The caller must convert ms→μs before calling
2. The signature changes to `latency_us: u64`

**Impact:** Changing to `u64` is a breaking API change for all callers of `request_ended`.

**Required fix:** Document whether `request_ended` signature changes or if conversion happens at the caller site.

---

## MEDIUM Issues

### MED-1: `ProviderBudgetLimancing` Typo

**Finding:** Mission line 4 says "ProviderBudgetLimancing disposition" — missing 'n'.

**Fix:** Correct to "ProviderBudgetLimiting".

---

### MED-2: `avg_latency` Still Returns `f64` in Tests

**Finding:** Test code (line 472) sets:
```rust
p.latencies = vec![100.0, 110.0, 105.0]; // Fast: ~105ms avg
```
And asserts against `avg_latency()` which returns `f64`. If we change to integer microseconds, this test literal must change too.

**Impact:** The test vectors use `f64` millisecond values. After migration to `u64` microseconds, all latency test values must be updated.

---

### MED-3: `CostBased` Currently Falls Back to `SimpleShuffle`

**Finding:** Code line 233:
```rust
RoutingStrategy::CostBased => Self::simple_shuffle_impl(providers), // Fallback
```

**RFC-0902 v1.3 line 88-89:**
> Route to cheapest provider (requires RFC-0904)

`CostBased` requires RFC-0904 for pricing data. Since RFC-0904 is not fully implemented, the fallback is correct. But the mission should note this is intentional placeholder behavior, not a bug.

---

## LOW Issues

### LOW-1: RFC Uses `ProviderState`, Code Uses `ProviderWithState`

**Finding:** RFC-0902 v1.3 line 197: `struct ProviderState`. Code line 87: `pub struct ProviderWithState`.

The names don't match. This is a pre-existing naming discrepancy. The mission correctly uses `ProviderWithState` (matching the code), but the RFC should perhaps be updated to match the code.

**No action required in this mission** — this is a separate RFC-0902 documentation issue.

---

### LOW-2: `Weighted` Display Implementation Missing

**Finding:** If `Weighted` is added to the enum, `Display` (lines 28-38) and `FromStr` (lines 41-54) must be updated. The mission doesn't mention these.

**Required:** Update `Display` and `FromStr` for `Weighted`.

---

## Structural Issues

### STRUCT-1: RFC Key Files Table Mismatch

**RFC-0902 v1.3 lines 329-333:**
| File | Change |
|------|--------|
| `crates/quota-router-cli/src/router.rs` | New - routing logic |
| `crates/quota-router-cli/src/config.rs` | Add router settings |
| `crates/quota-router-cli/src/providers.rs` | Add health checking |

**Actual code location:** `crates/quota-router-core/src/router.rs`

The RFC's "Key Files to Modify" section is stale — it references `quota-router-cli` but the routing code is in `quota-router-core`. This should be fixed in the RFC, not the mission.

---

## What the Mission Gets Right

1. ✅ Identifies `f64` → `u64` latency change requirement (with the storage design caveat above)
2. ✅ Identifies missing `success_count`/`total_count`
3. ✅ Identifies missing `Weighted` strategy
4. ✅ Identifies `ProviderBudgetLimiting` documentation need

---

## Required Fixes Before Mission Can Be Claimed

| Issue | Priority | Action |
|-------|----------|--------|
| CRITICAL-1 | MUST FIX | Update mission file path to `crates/quota-router-core/src/router.rs` |
| CRITICAL-2 | MUST FIX | Redesign latency storage — keep `Vec<u64>` samples + compute `avg_latency_us` on demand |
| HIGH-1 | MUST RESOLVE | Clarify if `Weighted` is distinct from `SimpleShuffle` |
| HIGH-2 | MUST SPECIFY | Define where `success_count`/`total_count` are incremented |
| HIGH-3 | MUST SPECIFY | Define `request_ended` signature change or caller-side conversion |
| MED-1 | MUST FIX | Fix "ProviderBudgetLimancing" typo → "ProviderBudgetLimiting" |
| LOW-2 | MUST ADD | Update `Display` and `FromStr` for new `Weighted` variant |

---

## Version History

| Version | Date | Changes |
|---------|------|---------|
| v1.0 | 2026-04-25 | Initial adversarial review |