# Mission: RFC-0909 compute_cost — Integer-Only Arithmetic

## Status

Open

## RFC

RFC-0909 v59 (Economics): Deterministic Quota Accounting

## Summary

Implement `compute_cost()` — a standalone function that computes total cost in micro-units using integer-only arithmetic. No floating point. Truncation error bounded at <2 micro-units per event.

## Acceptance Criteria

- [ ] `compute_cost(pricing: &PricingModel, input_tokens: u32, output_tokens: u32) -> u64`
- [ ] Standalone function (NOT a method on PricingTable or PricingModel)
- [ ] Integer-only: `(input_tokens as u64 * prompt_cost_per_1k / 1000) + (output_tokens as u64 * completion_cost_per_1k / 1000)`
- [ ] Uses `saturating_add` to prevent overflow
- [ ] Test vector: prompt_cost_per_1k=30_000, completion_cost_per_1k=60_000, input_tokens=100, output_tokens=50 → 6000 micro-units
- [ ] Truncation note documented: error bounded at <2 micro-units per event

## Implementation Notes

- Location: `crates/quota-router-core/src/keys/models.rs` (near `PricingModel`) or new `compute.rs` module
- `PricingModel` struct has fields: `model_name: String`, `prompt_cost_per_1k: u64`, `completion_cost_per_1k: u64`
- `compute_cost` takes `&PricingModel` (this RFC's type), NOT `&PricingTable` from RFC-0910
- Division is integer division (truncates toward zero)
- `saturating_add` prevents overflow (caps at u64::MAX ≈ $18M per event — overflow not realistic for token counts)

## Reference

- RFC-0909 §Cost Calculation
- RFC-0909 §compute_cost
- RFC-0909 §Truncation Note
- RFC-0909 §Overflow Safety

## Complexity

Low — straightforward integer arithmetic

---
**Mission Type:** Implementation
**Priority:** Critical
**Phase:** RFC-0909 Phase 1 Core
