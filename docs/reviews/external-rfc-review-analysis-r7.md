# External RFC Review Analysis — Round 7 Adversarial Review

**Reviewer:** External analysis (RFCs 0904 v1.27, 0910 v25, 0917 v2.18)
**Date:** 2026-04-25
**Internal analysis by:** Claude Code

---

## Preamble: Mission Status Violation

BLUEPRINT.md states: *"Missions REQUIRE an approved RFC — no RFC = no Mission = no implementation."*

All three missions are **Draft RFC**, not Accepted:

| Mission | RFC | Status |
|---------|-----|--------|
| `0904-a-cost-integration` | RFC-0904 v1.27 | **Draft** |
| `0910-a-pricing-table-registry` | RFC-0910 v25 | **Draft** |
| `0917-a-latency-tracker-alignment` | RFC-0917 v2.18 | **Draft** |

These missions are **not yet claimable** under BLUEPRINT.md rules. The external reviewer's BLOCK verdict is correct in spirit: implementation cannot proceed until RFCs are Accepted. This analysis documents findings for RFC authors to resolve before acceptance.

---

## Finding-by-Finding Analysis

### XH-1: Two `full` feature definitions

**External reviewer's claim:** Round 36 claimed to remove duplicate full feature TOML block at lines 111-123, but line 929 still has `full = ["litellm-mode", "any-llm-mode"]`.

**Technical analysis:**

Lines 127 and 133 define `full` in the §Rust Feature Gates section:
```toml
default = ["full"]
full = ["hyper", "axum", "py-o3"]  # line 133
```

Lines 926 and 929 define `full` in the §File Structure section:
```toml
default = ["full"]
full = ["litellm-mode", "any-llm-mode"]  # line 929
```

**Verdict: VALID.** Two conflicting `full` definitions exist. The §Rust Feature Gates section definition (line 133: `full = ["hyper", "axum", "py-o3"]`) is the normative one — it lists actual Cargo dependencies. The §File Structure definition (line 929: `full = ["litellm-mode", "any-llm-mode"]`) is a different, higher-level conceptual grouping that happens to use the same feature name. These are semantically different:
- Line 133 `full` = compile both HTTP proxy AND Python SDK (hyper/axum + py-o3)
- Line 929 `full` = compile both litellm-mode AND any-llm-mode

The cfg conditions at lines 586-591 correctly gate the struct field variants using the line 133 definition. But the duplicate name and the misleading "both strategies" comment on line 929 create confusion. **Fix required:** Rename the §File Structure feature to avoid name collision, e.g., `full-mode` or `dual-mode`, or remove it as redundant with the line 133 definition.

---

### XC-5: SpendEvent reads fields from ProviderResponse

**External reviewer's claim:** SpendEvent construction reads `response.pricing_hash`, `response.token_source`, `response.request_id` from `ProviderResponse` — fields that don't exist on that struct.

**Technical analysis:**

The external reviewer references RFC-0917's §Router Lifecycle (line 480 area). RFC-0917's `record_spend_atomic` signature is:
```rust
pub fn record_spend_atomic(
    storage: &dyn KeyStorage,
    event: &SpendEvent,
) -> Result<(), KeyError>
```

The `SpendEvent` is pre-constructed BEFORE being passed to `record_spend_atomic`. RFC-0917 does not show `SpendEvent` construction reading `ProviderResponse` fields. The external reviewer's claim confuses the caller (who constructs `SpendEvent`) with the callee (`record_spend_atomic`). The caller has access to all needed fields from the provider response context.

**Verdict: FORMAL REBUTTAL — reviewer misread the RFC.** The `record_spend_atomic` function receives a fully-constructed `SpendEvent`. It does not read fields from `ProviderResponse`. The external reviewer's concern would apply if `record_spend_atomic(response: &ProviderResponse)` was the signature — but it isn't. The SpendEvent is the caller's responsibility to construct with all required fields.

---

### NEW-C1: CostError variant shape mismatch

**External reviewer's claim:** RFC-0904 defines `CostError::Overflow { prompt_cost: u64, completion_cost: u64 }` (struct variant). RFC-0910 defines the same. But RFC-0909 (Accepted, v69) defines `CostError::Overflow` as a unit variant (no fields). Naming collision if both in scope.

**Technical analysis:**

RFC-0904 v1.27 lines 469-471 defines its own `CostError`:
```rust
pub enum CostError {
    Overflow { prompt_cost: u64, completion_cost: u64 },
}
```

RFC-0910 v25 defines `CostError` in its own module (`rfc0910::CostError`). RFC-0904's v1.27 changelog (line 1089) says: *"fix XC-3 (compute_cost: removed duplicate implementation; delegates to rfc0910::compute_cost per §Cost Computation; added From<CostError> for BudgetError)"* — so RFC-0904 now DELEGATES to RFC-0910's compute_cost and converts errors.

The Accepted RFC-0909 v69 does not define `CostError` at all — it references `BudgetError` from RFC-0904. The external reviewer incorrectly claims "RFC-0909 v65 defines `CostError::Overflow` as a unit variant." RFC-0909 is a quota accounting RFC, not a cost computation RFC — it does not define cost error types.

**Verdict: FORMAL REBUTTAL — external reviewer incorrectly identified RFC-0909 as defining CostError.** The actual concern about `CostError` variants should be directed at RFC-0904 (which defines it) and RFC-0910 (which also defines it). Since RFC-0904 delegates to RFC-0910, only RFC-0910's `CostError` should be the canonical definition. This is an internal consistency issue between draft RFCs, not a cross-RFC collision with the Accepted RFC-0909.

**However:** the structural concern is valid as an internal RFC consistency issue. RFC-0904 should NOT define its own `CostError` if it delegates to RFC-0910 — it should import RFC-0910's `CostError` type. **Fix required:** RFC-0904 should remove its own `CostError` definition and import from RFC-0910, or RFC-0910 should export `CostError` for RFC-0904 to use.

---

### XC-1 + XC-2: Unresolved in RFC-0909 v65

**External reviewer's claim:** These remain unresolved in RFC-0909 v65.

---

## Formal Rebuttal: XC-1 and XC-2 Were Already Fixed in v66

The external reviewer examined RFC-0909 **v65** and concluded XC-1/XC-2 were unresolved. This is factually incorrect. Both issues were fixed in **v66 (Round 35, 2026-04-24)**, one round before the reviewer's base version.

### XC-1: InternalPricingTable.compute_pricing_hash removed

**What the issue was:**
RFC-0909 v65 still had an `InternalPricingTable` struct with a `compute_pricing_hash()` method. This was problematic because:
1. RFC-0909 is a quota accounting RFC, not a pricing table RFC — pricing_hash computation belongs to RFC-0910
2. The struct was named `PricingTable` originally (then renamed to `InternalPricingTable` in v60 to avoid collision with RFC-0910's canonical `PricingTable`)
3. Having `compute_pricing_hash()` on `InternalPricingTable` implied RFC-0909 defines the pricing_hash mechanism

**What the fix did (v66, line 1675):**
> *"fix XC-1 (remove InternalPricingTable.compute_pricing_hash — RFC-0909 no longer defines pricing_hash; caller must obtain via RFC-0910 PricingRegistry::get(...).compute_pricing_hash())"*

**Verification — current RFC-0909 v69 (lines 776-837):**
```rust
impl InternalPricingTable {
    pub fn new() -> Self { /* ... */ }

    /// Look up pricing for a model
    pub fn get(&self, model: &str) -> Option<&PricingModel> {
        self.models.get(model)
    }

    /// Compute SHA256 pricing hash for this table snapshot
    /// Used in event_id to tie costs to specific pricing version
    /// **Merkle leaf requirement:** ... MUST use DCS (Entry 16, Part 3) binary encoding — NOT JSON.
    pub fn models(&self) -> impl Iterator<Item = &PricingModel> {
        self.models.values()
    }
}
```

**No `compute_pricing_hash()` method exists.** The only methods are `new()`, `get()`, and `models()`. The problematic method was removed. The struct now serves only as a pricing lookup table for cost computation (line 1073: `let pricing = PRICING_TABLE.get(model)`), not as a pricing_hash source.

The comment at line 947 explicitly defers pricing_hash authority to RFC-0910:
> *"RFC-0910 will provide immutable pricing table snapshots."*

And the `process_response` pseudocode at line 1057 confirms the caller-side computation:
> `pricing_hash: [u8; 32], // obtained by: PricingRegistry::get(provider, model)?.compute_pricing_hash() per RFC-0910 §PricingTable.compute_pricing_hash — caller computes this before calling process_response`

---

### XC-2: process_response pricing_hash comment corrected

**What the issue was:**
The `process_response` pseudocode at line ~1073 originally said:
```rust
let pricing = PRICING_TABLE.get(model);
let cost_amount = compute_cost(pricing, ...);
// pricing_hash was then derived from pricing somehow — the comment was stale
```

The comment implied `PricingModel` had a `compute_pricing_hash()` method — it does not and cannot (PricingModel is a single model entry, not a full pricing table). The pricing_hash must come from the caller's RFC-0910 lookup, not from the pricing lookup.

**What the fix did (v66, line 1675):**
> *"fix XC-2 (update process_response pricing_hash comment: PRICING_TABLE.get(model).compute_pricing_hash() was invalid call, PricingModel has no such method — now documents correct call path)"*

**Verification — current RFC-0909 v69 (lines 1050-1105):**
```rust
pub async fn process_response(
    db: &Database,
    key_id: &uuid::Uuid,
    team_id: Option<&uuid::Uuid>,
    provider: &str,
    model: &str,
    response: &ProviderResponse,
    pricing_hash: [u8; 32], // ←-pricing_hash is a PARAMETER, not derived internally
) -> Result<(), KeyError> {
    // ...
    // 3. Look up pricing (should be cached singleton in production — see §InternalPricingTable Caching)
    let pricing = PRICING_TABLE.get(model).ok_or(KeyError::NotFound)?;
    // pricing_hash is used directly at step 4 — NOT derived from pricing
    let event_id = compute_event_id(
        &response.request_id, key_id, provider, model,
        response.input_tokens, response.output_tokens,
        &pricing_hash,  // ← passed in, used directly
        token_source,
    );
```

The pseudocode now shows `pricing_hash` as an **input parameter** (line 1057), not as something derived from `PRICING_TABLE.get(model)`. The caller's responsibility is documented explicitly in the parameter comment:
> *"obtained by: PricingRegistry::get(provider, model)?.compute_pricing_hash() per RFC-0910 §PricingTable.compute_pricing_hash — caller computes this before calling process_response"*

This is a **correct call path**: caller → RFC-0910 → pricing_hash → process_response as parameter.

---

### Summary of Fix Evidence

| Evidence | Location | Shows |
|----------|----------|-------|
| InternalPricingTable no longer has `compute_pricing_hash()` | Lines 776-837 | XC-1 fixed |
| process_response receives pricing_hash as parameter | Line 1057 | XC-2 fixed |
| Parameter comment explicitly names RFC-0910 as source | Line 1057 | XC-2 fixed |
| Comment defers pricing_hash authority to RFC-0910 | Line 947 | XC-1 fixed |
| Version history v66 explicitly lists both fixes | Line 1675 | XC-1+XC-2 closed |

**Verdict: ALREADY FIXED in v66 (2026-04-24).** External reviewer's base version was v65. No action needed.

---

### NEW-C2: octo_w_balances missing FK to api_keys

**External reviewer's claim:** The DDL defines `key_id BLOB(16) NOT NULL PRIMARY KEY` with no `FOREIGN KEY (key_id) REFERENCES api_keys(key_id) ON DELETE CASCADE`. Orphan rows possible on key deletion.

**Technical analysis:**

This DDL is in RFC-0904 v1.27 §OCTO-W Interface (around line DDL area). The external reviewer is technically correct — a PRIMARY KEY that is also a foreign key should have an ON DELETE CASCADE referential action to prevent orphaned balance rows when API keys are deleted.

**Verdict: VALID.** Add `FOREIGN KEY (key_id) REFERENCES api_keys(key_id) ON DELETE CASCADE` to the `octo_w_balances` DDL. This is a legitimate database integrity issue.

---

## Summary

| Finding | Verdict | Action Required |
|---------|---------|-----------------|
| XH-1: Two conflicting `full` definitions | **VALID** | Rename one: `full` in §File Structure (line 929) → `full-mode` or remove |
| XC-5: SpendEvent reads non-existent ProviderResponse fields | **FORMAL REBUTTAL** | External reviewer misread — `record_spend_atomic` receives pre-constructed SpendEvent |
| NEW-C1: CostError variant mismatch | **FORMAL REBUTTAL + valid concern** | Rebuttal: RFC-0909 doesn't define CostError. Valid: RFC-0904 should not define its own CostError if delegating to RFC-0910 |
| XC-1 + XC-2: unresolved in RFC-0909 | **ALREADY FIXED** | v66 (2026-04-24) already closed both XC-1 and XC-2. External reviewer was reading v65. |
| NEW-C2: octo_w_balances missing FK | **VALID** | Add `FOREIGN KEY (key_id) REFERENCES api_keys(key_id) ON DELETE CASCADE` |

---

## Mission Status Conclusion

Per BLUEPRINT.md: *"Missions REQUIRE an approved RFC."* These three missions (`0904-a`, `0910-a`, `0917-a`) are **not claimable** until their RFCs are Accepted. The external review BLOCK is valid in that the RFCs are not yet ready for implementation.

**RFC readiness for acceptance:**

| RFC | Ready? | Blocking issues |
|-----|--------|----------------|
| RFC-0904 | No | Two `full` definitions (rename one); CostError self-definition (remove, import from RFC-0910); FK on octo_w_balances |
| RFC-0910 | Unclear | External review focused on RFC-0904 integration; RFC-0910 CostError definition needs verification |
| RFC-0917 | No | Two `full` definitions (XH-1); cost error type delegation to RFC-0904 not yet established |

**Required before acceptance:** RFC authors must resolve XH-1, NEW-C2, and the CostError delegation issue across the three RFCs.
