# Adversarial Review Round 6: Mission 0902-e — RFC-0902 v1.5 Alignment

**Reviewer:** Code Review Agent
**Date:** 2026-04-25
**Mission:** `missions/open/0902-e-routing-metrics-alignment.md` (v6, post-R5 fixes)
**RFC:** RFC-0902 v1.5 (Accepted)
**Code:** `crates/quota-router-core/src/router.rs`

---

## Executive Summary

| Severity | Count |
|----------|-------|
| CRITICAL | 0 |
| HIGH | 1 |
| MEDIUM | 1 |
| LOW | 1 |

All R5 fixes correctly applied. This round finds one consistency issue: the RFC YAML comment says "model_name → weight" but the keys in the YAML example are provider names ("openai", "anthropic"). The mission implementation uses `provider.name` as key (correct), but the RFC YAML comment is stale. This is a pre-existing RFC issue, not a mission bug.

---

## HIGH Issues

### HIGH-1: RFC YAML Comment — "model_name → weight" But Keys Are Provider Names

**Finding:** RFC-0902 YAML example (lines 143-149):

```yaml
# Provider weights for weighted routing (GLOBAL weights map — model_name → weight)
# NOT per-provider weight. Weighted strategy uses this global map to select providers.
# If a model_name is not in weights, falls back to SimpleShuffle behavior (rpm-based).
weights:
  openai: 10
  anthropic: 5
  google: 3
```

**Problem:** The comment says "GLOBAL weights map — model_name → weight" but the YAML keys ("openai", "anthropic", "google") are **provider names**, not model names.

- `model_name` would be "gpt-3.5-turbo", "claude-3-opus", etc.
- `provider.name` would be "openai", "anthropic", "azure", "google", etc.

The keys in the YAML example are provider names. So the comment "model_name → weight" is wrong — it should say "provider.name → weight".

**Impact on mission:** The mission implementation (lines 112-113) correctly uses `provider.name` as the key:
```rust
/// Global weights map for Weighted strategy: provider.name → weight
```

The mission is internally consistent and correct. The RFC YAML comment is stale.

**RFC vs Mission alignment:** The RFC comment is a pre-existing documentation bug in the RFC. The mission correctly implements the RFC's intent (global weights map, not per-provider) using `provider.name`. No change needed to the mission.

**Fix required for RFC:** Update RFC-0902 YAML comment from "model_name → weight" to "provider.name → weight".

---

## MEDIUM Issues

### MED-1: Mission Doesn't Mention Updating RFC YAML Comment

**Finding:** The mission mentions updating the RFC (line 17: "Update `crates/quota-router-core/src/router.rs` to match RFC-0902 v1.5 changes") but doesn't explicitly call out the RFC YAML comment inconsistency as something to fix.

The mission is correctly focused on code implementation. However, given the "ALWAYS SOLVE ALL ISSUES" rule in memory, and given that this RFC issue is found during the mission's review cycle, should the mission document this RFC bug?

**Decision:** No. The mission is a code implementation document, not an RFC correction document. The RFC issue should be fixed separately (via RFC update process). The mission's implementation is consistent and correct — it uses `provider.name` as the key.

**Status:** Not a mission bug. RFC documentation issue.

---

## LOW Issues

### LOW-1: Round-Robin Index Initialization — No Reset on Provider Changes

**Finding:** Code (lines 175-176):
```rust
// Initialize round-robin indices
let round_robin_index = providers_map.keys().map(|k| (k.clone(), 0)).collect();
```

Round-robin indices are initialized once when Router is created. If providers are dynamically added/removed (future feature), the round_robin_index doesn't update.

**Status:** Pre-existing design. Not introduced by mission. Not a bug.

---

## Pre-Existing Issues (Not Introduced by Mission)

### KNOWN-1: RFC Uses `ProviderState`, Code Uses `ProviderWithState`

Pre-existing naming discrepancy. No action in this mission.

### KNOWN-2: LiteLLM RoutingStrategy Enum Missing `Weighted`

LiteLLM doesn't have `Weighted`. RFC custom addition. Mission correctly implements RFC spec.

### KNOWN-3: RFC YAML Comment — "model_name → weight" Should Be "provider.name → weight"

RFC-0902 lines 143-149. Pre-existing documentation bug. The mission implementation is correct (uses `provider.name`). RFC should be updated separately.

---

## What Was Fixed in R5 ✅

| Issue | Status |
|-------|--------|
| HIGH-2 (R5): RouterConfig comment says "model_name → weight" | ✅ Fixed — changed to "provider.name → weight" |
| MED-1 (R5): Stale v1.3 reference | ✅ Already fixed — AC correctly says v1.5 |

---

## Required Fixes Summary

| Issue | Priority | Fix | Status |
|-------|----------|-----|--------|
| HIGH-1: RFC YAML comment "model_name → weight" | RFC FIX | ✅ Fixed — RFC-0902 v1.6 updated comment to "provider.name → weight" | ✅ Fixed |

**Note:** Mission implementation is internally consistent and correct. RFC YAML comment was stale documentation — fixed in RFC-0902 v1.6.

---

## Version History

| Version | Date | Changes |
|---------|------|---------|
| v1.0 | 2026-04-25 | Initial Round 6 adversarial review — 0 CRITICAL, 1 HIGH, 1 MED, 1 LOW |
| v1.1 | 2026-04-25 | Add MED-1 note (mission doesn't need to fix RFC YAML comment); clarify mission is correct, RFC has stale comment |