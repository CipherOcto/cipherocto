# Mission: RFC-0909 compute_cost — Integer-Only Arithmetic

## Status

Open (v3)

## RFC

RFC-0909 v59 (Economics): Deterministic Quota Accounting

## Summary

Implement `compute_cost()` — a standalone function that computes total cost in micro-units using integer-only arithmetic. No floating point. Truncation error bounded at <2 micro-units per event.

## Acceptance Criteria

- [ ] `PricingModel` struct: `#[derive(Debug, Clone, Serialize, Deserialize)]` with fields `model_name: String`, `prompt_cost_per_1k: u64`, `completion_cost_per_1k: u64` (per RFC-0909 §PricingModel)
- [ ] `compute_cost(pricing: &PricingModel, input_tokens: u32, output_tokens: u32) -> u64`
- [ ] Standalone function (NOT a method on PricingTable or PricingModel)
- [ ] Integer-only formula (H2): `let prompt_cost = (input_tokens as u64 * pricing.prompt_cost_per_1k) / 1000; let completion_cost = (output_tokens as u64 * pricing.completion_cost_per_1k) / 1000; prompt_cost.saturating_add(completion_cost)` — two-step computation matching RFC pseudocode structure
- [ ] Uses `saturating_add` for local addition (single-request overflow is impossible; see note below)
- [ ] Test vector: `let pricing = PricingModel { model_name: "test".into(), prompt_cost_per_1k: 30_000, completion_cost_per_1k: 60_000 }; assert_eq!(compute_cost(&pricing, 100, 50), 6000);`
- [ ] Truncation note documented: error bounded at <2 micro-units per event

## Implementation Notes

- Location: `crates/quota-router-core/src/keys/models.rs` or new `compute.rs` module
- `PricingModel` struct: `model_name: String`, `prompt_cost_per_1k: u64`, `completion_cost_per_1k: u64`
- Division is integer division (truncates toward zero). For micro-unit pricing, truncation occurs only when cost < 0.5 micro-units — effectively free. Error is bounded at <2 micro-units per event (H1).
- 1000 = TOKEN_SCALE (micro-units per token)
- `saturating_add`: local per-event cost computation cannot overflow (would require >1.8×10¹⁹ tokens in a single request). This differs from `record_spend` budget accumulation which uses checked arithmetic (per RFC-0909 §Overflow Safety)
- `compute_cost` takes `&PricingModel` (RFC-0909's type), NOT `&PricingTable` from RFC-0910

## Reference

- RFC-0909 §Cost Calculation
- RFC-0909 §compute_cost
- RFC-0909 §Truncation Note
- RFC-0909 §Overflow Safety (checked_add vs saturating_add distinction)
- RFC-0201 §Integer Scaling (TOKEN_SCALE = 1000)

## Dependencies

- `serde = "1.x"` (with `derive` feature) for Serialize/Deserialize derives on PricingModel

## Complexity

Low — straightforward integer arithmetic

---
**Mission Type:** Implementation
**Priority:** Critical
**Phase:** RFC-0909 Phase 1 Core

## Changelog

| Version | Date | Changes |
|---------|------|---------|
| v3 | 2026-04-20 | Round 2 adversarial review fixes: fix C1 (add Serialize/Deserialize derives to PricingModel); fix H1 (add truncation context — error <0.5 micro-units per step); fix H2 (show two-step computation matching RFC pseudocode); fix M1 (add RFC-0201 §Integer Scaling reference); fix M2 (add serde dependency); fix L1 (show actual assert_eq! test code) |
| v2 | 2026-04-20 | Round 1 adversarial review fixes: fix C1 (add PricingModel struct to acceptance criteria); fix H1 (add saturating_add vs checked arithmetic distinction note); fix M1 (explicit test assertion); fix M2 (add TOKEN_SCALE micro-unit note) |
| v1 | 2026-04-20 | Initial |
