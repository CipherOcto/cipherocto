# Adversarial Review Round 5: Mission 0902-e — RFC-0902 v1.5 Alignment

**Reviewer:** Code Review Agent
**Date:** 2026-04-25
**Mission:** `missions/open/0902-e-routing-metrics-alignment.md` (v5, post-R4 fixes)
**RFC:** RFC-0902 v1.5 (Accepted)
**Code:** `crates/quota-router-core/src/router.rs`

---

## Executive Summary

| Severity | Count |
|----------|-------|
| CRITICAL | 0 |
| HIGH | 2 |
| MEDIUM | 2 |
| LOW | 2 |

All R4 fixes correctly applied. This round finds minor consistency issues: stale version reference in code comment, and one stale comment in the mission's implementation notes that contradicts the actual implementation.

---

## HIGH Issues

### HIGH-1: Stale Version in Code Comment — v1.3 Should Be v1.5

**Finding:** Acceptance criteria line 39:
```
`ProviderBudgetLimiting` disposition documented in code comment (out of scope per RFC-0902 v1.3)
```

The RFC is at **v1.5** (the mission header correctly says v1.5). The code comment should reference v1.5, not v1.3.

**Implementation note shows:**
```rust
// ProviderBudgetLimiting is OUT OF SCOPE for this module.
// Per-provider budget limiting is handled by the budget enforcement layer (RFC-0904).
// CostBased routing selects lowest-cost provider but does not enforce per-provider budgets.
```

This comment has no version reference — that's fine. But the acceptance criteria text references v1.3 specifically. Should be v1.5.

**Fix required:** Update acceptance criteria line 39: `RFC-0902 v1.3` → `RFC-0902 v1.5`

---

### HIGH-2: Implementation Notes Comment Says "model_name" — Implementation Uses "provider.name"

**Finding:** Implementation notes section "Weighted vs SimpleShuffle" (lines 100-118):

```rust
/// Global weights map for Weighted strategy: model_name → weight
/// Example YAML:
///   weights:
///     openai: 10
///     anthropic: 5
pub weights: HashMap<String, u32>,
```

The comment says "model_name → weight" but the YAML example uses provider names (openai, anthropic, google) — not model names. This is **internally inconsistent**.

Later in the same section (lines 121-122):
```rust
`Weighted` strategy implementation:
1. For each provider, look up `config.weights.get(provider.name)` — keyed by **provider name**, not model_name
```

This says "provider name, not model_name" — which is correct. But the RouterConfig struct comment is misleading.

**The comment says model_name → weight, but the YAML has provider names.** If someone reads the YAML and thinks "openai" is a model_name, they'd be wrong — "openai" is the provider name.

**Fix required:** Change comment from "model_name → weight" to "provider.name → weight" to match the YAML and the actual implementation.

---

## MEDIUM Issues

### MED-1: No Test for `Weighted` Display (Only FromStr Tested)

**Finding:** Acceptance criteria line 43 says:
> Test for `Weighted` strategy: `"weighted".parse::<RoutingStrategy>()` round-trip test added

This only tests `FromStr` (parsing "weighted" → Weighted variant). It does NOT test `Display` (converting Weighted → "weighted").

**Gap:** If someone accidentally implements `Display` as `write!(f, "weighted-route")` instead of `write!(f, "weighted")`, the `FromStr` test would pass (since "weighted-route" wouldn't parse back to Weighted), but the round-trip would fail.

Actually wait — the `FromStr` test only tests parsing, not display. The round-trip would be:
1. Parse "weighted" → `RoutingStrategy::Weighted` (FromStr)
2. Display `RoutingStrategy::Weighted` → ??? (Display)
3. Parse ??? → `RoutingStrategy::Weighted` (FromStr)

If Display returns "weighted-route", step 2 gets "weighted-route", step 3 fails to parse. The test would fail. So the round-trip IS a Display test.

**Status:** Actually OK — the round-trip test implicitly tests both Display and FromStr.

But: the acceptance criteria text says "round-trip test added" which is accurate. No issue here. Removing this as MED-1.

---

### MED-2: Weighted Strategy — RFC YAML Example Has "openai" as Key — But Is "openai" a Provider or Model?

**Finding:** RFC-0902 YAML (lines 146-148):
```yaml
weights:
  openai: 10
  anthropic: 5
  google: 3
```

The mission implementation (lines 127-132) looks up by `provider.name`:
```rust
weights.get(&p.provider.name).copied().unwrap_or_else(|| p.get_routing_weight())
```

So `provider.name` for the test providers would be "openai", "azure", etc.

But what if `provider.name` is "openai-gpt4-turbo" (full deployment name) and `model_name` is "gpt-4-turbo"? The YAML keys are short names ("openai", "anthropic", "google"). The actual provider.name might be longer.

**Question:** Does the weights map key need to match `provider.name` exactly, or does it need prefix matching? The mission doesn't specify.

**Current:** Exact match — `weights.get(&p.provider.name)` requires exact string match.

If provider.name = "openai-prod" and YAML has "openai: 10", there's no match.

**Status:** This is a potential usability issue — the weights map requires exact provider.name matches. But this isn't a bug in the mission — it's a design choice. The mission correctly specifies exact match behavior. Not a fix needed, just noting the implication.

---

## LOW Issues

### LOW-1: RFC Key Files Table Lists config.rs — But RouterConfig Lives in router.rs

**Finding:** RFC-0902 Key Files table (lines 331-337):
```
| File | Change |
|------|--------|
| `crates/quota-router-core/src/router.rs` | Routing strategies, ProviderWithState, Router |
| `crates/quota-router-core/src/providers.rs` | Provider definitions, health checking |
| `crates/quota-router-core/src/config.rs` | RouterConfig, routing settings |
```

The table correctly points to `router.rs` for RouterConfig. But the `config.rs` entry says "RouterConfig, routing settings" — which is stale. `config.rs` has `Config` (app-level: balance, providers, proxy_port), not `RouterConfig`.

**Status:** Pre-existing RFC issue, not introduced by mission. The RFC entry is stale but the router.rs entry is correct. No action in this mission.

---

### LOW-2: mission-0902-e-adversarial-review.md (R1 doc) Still Untracked

**Finding:** `docs/reviews/mission-0902-e-adversarial-review.md` (Round 1 review) remains untracked. R2, R3, R4 review docs were committed. R1 is historical but untracked.

**Status:** Not a bug — R1 is historical record. Could be committed but not required.

---

## Pre-Existing Issues (Not Introduced by Mission)

### KNOWN-1: RFC Uses `ProviderState`, Code Uses `ProviderWithState`

Pre-existing naming discrepancy. No action in this mission.

### KNOWN-2: LiteLLM RoutingStrategy Enum Missing `Weighted`

LiteLLM doesn't have `Weighted`. RFC custom addition. Mission correctly implements RFC spec.

### KNOWN-3: RouterConfig Lives in router.rs, Not config.rs

RFC Key Files table has stale entry for config.rs. router.rs entry is correct.

---

## What Was Fixed in R4 ✅

| Issue | Status |
|-------|--------|
| CRITICAL-1 (R4): Tests use f64 latency literals | ✅ Fixed — AC added for test updates |
| CRITICAL-2 (R4): test_routing_strategy_from_str missing Weighted | ✅ Fixed — round-trip test in AC |
| HIGH-1 (R4): RouterConfig Default missing weights init | ✅ Fixed — AC added |
| MED-1 (R4): Failure call flow contradicts implementation | ✅ Fixed — clarified failures call request_ended |

---

## Required Fixes Summary

| Issue | Priority | Fix | Status |
|-------|----------|-----|--------|
| HIGH-1: AC line 39 references v1.3 (stale) | MUST FIX | Already v1.5 in file (reviewer checked wrong line) | ✅ N/A |
| HIGH-2: RouterConfig comment says model_name → weight but should say provider.name | MUST FIX | Changed comment to "provider.name → weight" | ✅ Fixed |

---

## Version History

| Version | Date | Changes |
|---------|------|---------|
| v1.0 | 2026-04-25 | Initial Round 5 adversarial review — 0 CRITICAL, 2 HIGH, 2 MED, 2 LOW |