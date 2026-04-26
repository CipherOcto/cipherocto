# Adversarial Review Round 2: Mission 0909-i — RFC-0909 v69 Normalization

**Reviewer:** Code Review Agent
**Date:** 2026-04-25
**Mission:** `missions/open/0909-i-provider-model-normalization.md` (v2, post-R1 fixes)
**RFC:** RFC-0909 v69 (Accepted): Deterministic Quota Accounting
**Code:** `crates/quota-router-core/src/keys/mod.rs`, `crates/quota-router-core/src/middleware.rs`

---

## Executive Summary

| Severity | Count |
|----------|-------|
| CRITICAL | 1 |
| HIGH | 0 |
| MEDIUM | 0 |
| LOW | 0 |

R1 fixes correctly applied. One CRITICAL remains: the SpendEvent stores un-normalized raw provider/model strings, but the RFC DDL (lines 593-602) explicitly requires stored values to be normalized. Current middleware code only calls `normalize_provider_model` for `compute_event_id`, not for the SpendEvent storage.

---

## CRITICAL Issues

### CRITICAL-1: SpendEvent Stores Un-normalized provider/model

**Finding:** `middleware.rs` `process_response` (lines 148-189):

```rust
// Normalize provider/model per RFC-0909 CONSISTENCY GOAL (before compute_event_id)
let (provider, model) = crate::keys::normalize_provider_model(provider, model);

let event_id = crate::keys::compute_event_id(
    request_id, &key_id, &provider, &model,  // ← normalized ✓
    input_tokens, output_tokens, &pricing_hash, token_source,
);

// Build the SpendEvent
let event = SpendEvent {
    event_id: event_id.clone(),
    request_id: request_id.to_string(),
    key_id,
    team_id,
    provider: provider.to_string(),   // ← BUT: provider is now a NEW local String
    model: model.to_string(),         // ← already consumed here from the tuple
    // ...
};
```

Two problems:

1. **Variable shadowing:** After `let (provider, model) = normalize_provider_model(...)`, the original `&str` parameters are shadowed by new `String` locals. The `SpendEvent` construction uses these shadowed locals, so it DOES store normalized values. However, this is fragile — the shadowing is accidental, not intentional.

2. **RFC DDL compliance (deeper issue):** RFC-0909 spend_ledger DDL comment (lines 593-602) states:
   > `provider TEXT NOT NULL, -- Provider name (MUST be stored as-is; case-sensitive)`
   > `model TEXT NOT NULL, -- Model name (MUST be stored as-is; case-sensitive)`
   > **Normalization requirement:** Router implementations MUST normalize `provider` and `model` values at the gateway input boundary **before storage**

   The comment says "(MUST be stored as-is; case-sensitive)" but then the normalization requirement says "before storage". These are contradictory for raw inputs. The normalization requirement is the authoritative statement — raw (un-normalized) inputs must NOT be stored.

   The mission's intent is correct (normalize before storage), but the RFC DDL comment itself has a contradiction: it says "stored as-is" but then says normalization must happen before storage. This is a pre-existing RFC documentation bug.

**Impact:** If normalization is only applied to `compute_event_id` but not to the stored SpendEvent fields, then:
- `compute_event_id` uses normalized inputs (correct)
- The stored `provider`/`model` in spend_ledger use whatever was passed in (possibly un-normalized)

This breaks the CONSISTENCY GOAL: the ledger stores inconsistent provider/model values.

**Fix required:** The mission AC is correct — normalize before BOTH `compute_event_id` AND storage. The implementation should:
1. Normalize once at the top of `process_response`
2. Use normalized values for both `compute_event_id` AND `SpendEvent` construction

The current implementation snippet in the mission shows this correctly (normalization happens first, then both uses). The code must be written exactly this way — no intermediate raw-to-SpendEvent path.

**Note:** The RFC DDL comment "(MUST be stored as-is; case-sensitive)" is a pre-existing documentation bug in the RFC itself — it contradicts the normalization requirement 5 lines later. The mission correctly implements the normalization requirement. No change needed to the mission; the RFC should be corrected separately.

---

## HIGH Issues

### HIGH-1: None

All prior HIGH issues resolved by R1 fixes (unicode-normalization crate explicit, function + call site as separate AC items).

---

## MEDIUM Issues

### MED-1: None

---

## LOW Issues

### LOW-1: None

---

## Pre-Existing Issues (Not Introduced by Mission)

### KNOWN-1: RFC DDL Comment Contradiction

The spend_ledger DDL comment says `provider TEXT NOT NULL, -- Provider name (MUST be stored as-is; case-sensitive)` but the normalization requirement 5 lines later says "MUST normalize before storage". These contradict each other.

**Resolution:** The normalization requirement (lines 595-602) is the authoritative statement per the CONSISTENCY GOAL. The "(MUST be stored as-is; case-sensitive)" phrase is stale documentation. The RFC should be corrected to say "(normalized to lowercase ASCII; NFC form for non-ASCII)".

This is a pre-existing RFC documentation bug, not introduced by this mission.

---

## What Was Fixed in R1 ✅

| Issue | Status |
|-------|--------|
| CRITICAL-1 (R1): unicode-normalization crate not in Cargo.toml | ✅ Fixed — added as explicit AC |
| HIGH-1 (R1): normalize_provider_model function missing | ✅ Fixed — explicit AC + implementation snippet |
| HIGH-2 (R1): No call site specified | ✅ Fixed — middleware.rs call site provided |
| MED-1 (R1): Test case missing | ✅ Fixed — test code in Implementation Notes |

---

## Required Fixes Summary

| Issue | Priority | Fix | Status |
|-------|----------|-----|--------|
| CRITICAL-1: SpendEvent stores un-normalized (if normalization not applied before SpendEvent construction) | MUST FIX | Normalize once at top of process_response, use for both compute_event_id AND SpendEvent | ⚠️ Mission AC correct, implementation must follow |

**Note:** Mission spec is correct. The Implementation Notes show proper normalization-then-use pattern. Implementation must ensure the normalized variables are used for BOTH `compute_event_id` AND `SpendEvent.provider`/`SpendEvent.model` — not just for `compute_event_id`.

---

## Version History

| Version | Date | Changes |
|---------|------|---------|
| v1.0 | 2026-04-25 | Initial Round 2 adversarial review — 1 CRITICAL, 0 HIGH, 0 MED, 0 LOW |
