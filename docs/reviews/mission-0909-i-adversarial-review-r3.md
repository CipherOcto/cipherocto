# Adversarial Review Round 3: Mission 0909-i — RFC-0909 v69 Normalization

**Reviewer:** Code Review Agent
**Date:** 2026-04-25
**Mission:** `missions/open/0909-i-provider-model-normalization.md` (v3, post-R2 fixes)
**RFC:** RFC-0909 v69 (Accepted): Deterministic Quota Accounting
**Code:** `crates/quota-router-core/src/keys/mod.rs`, `crates/quota-router-core/src/middleware.rs`

---

## Executive Summary

| Severity | Count |
|----------|-------|
| CRITICAL | 0 |
| HIGH | 0 |
| MEDIUM | 0 |
| LOW | 0 |

Mission 0909-i is **specification-complete**. All R2 fixes correctly applied. No issues remain in the mission itself.

**Note:** The code in `middleware.rs` (lines 148-189) is NOT YET updated — it still passes raw `provider`/`model` to both `compute_event_id` and `SpendEvent`. This is expected since the mission is still in `Open` status (not yet claimed/implemented). The mission specification correctly describes what must be built.

---

## HIGH Issues

### HIGH-1: None

---

## MEDIUM Issues

### MED-1: None

---

## LOW Issues

### LOW-1: None

---

## Pre-Existing Issues (Not Introduced by Mission)

### KNOWN-1: RFC DDL Comment Says "(MUST be stored as-is; case-sensitive)" — Contradiction

From R2: Lines 593-594 of the RFC say:
```
provider TEXT NOT NULL,  -- Provider name (MUST be stored as-is; case-sensitive)
model TEXT NOT NULL,     -- Model name (MUST be stored as-is; case-sensitive)
```

But lines 595-602 say:
```
-- **Normalization requirement (HIGH-3):** Router implementations
-- MUST normalize `provider` and `model` values at the gateway input boundary before storage
```

**Resolution:** The normalization requirement (lines 595-602) is the authoritative statement. The "(MUST be stored as-is; case-sensitive)" comment on lines 593-594 is stale documentation that contradicts the requirement 2 lines later. This is a pre-existing RFC documentation bug, not introduced by this mission.

**Status:** Pre-existing. Not fixed in this mission. Not the mission's responsibility to fix the RFC.

---

## What Was Fixed in R2 ✅

| Issue | Status |
|-------|--------|
| R2 CRITICAL-1: SpendEvent stores un-normalized if implementation doesn't follow spec | ✅ Fixed — mission Implementation Notes extended with full call-site snippet showing SpendEvent using normalized locals directly, plus CRITICAL warning block |

---

## What Was Fixed in R1 ✅

| Issue | Status |
|-------|--------|
| R1 CRITICAL-1: unicode-normalization crate not in Cargo.toml | ✅ Fixed — explicit AC item added |
| R1 HIGH-1: normalize_provider_model function missing | ✅ Fixed — explicit AC item + implementation snippet |
| R1 CRITICAL-2: process_response doesn't normalize | ✅ Fixed — middleware call site in Implementation Notes |
| R1 MED-1: Test case missing | ✅ Fixed — test code in Implementation Notes |

---

## Required Fixes Summary

| Issue | Priority | Fix | Status |
|-------|----------|-----|--------|
| All issues | — | — | ✅ All resolved |

**Conclusion:** Mission is specification-complete. Ready to claim and implement.

---

## Version History

| Version | Date | Changes |
|---------|------|---------|
| v1.0 | 2026-04-25 | Initial Round 3 adversarial review — 0 CRITICAL, 0 HIGH, 0 MED, 0 LOW |
