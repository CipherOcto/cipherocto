# Mission: RFC-0909 compute_cost — Integer-Only Arithmetic

## Status

Completed (v5)

## Claimant

@mmacedoeu

## Pull Request

#

## RFC

RFC-0909 v59 (Economics): Deterministic Quota Accounting

## Summary

Implement `compute_cost()` — a standalone function that computes total cost in micro-units using integer-only arithmetic. No floating point. Truncation error bounded at <2 micro-units per event.

## Acceptance Criteria

- [x] `PricingModel` struct: `#[derive(Debug, Clone, Serialize, Deserialize)]` with fields `model_name: String`, `prompt_cost_per_1k: u64`, `completion_cost_per_1k: u64` (per RFC-0909 §PricingModel)
- [x] `compute_cost(pricing: &PricingModel, input_tokens: u32, output_tokens: u32) -> u64`
- [x] Standalone function (NOT a method on PricingTable or PricingModel)
- [x] Integer-only formula (H2): `let prompt_cost = (input_tokens as u64 * pricing.prompt_cost_per_1k) / 1000; let completion_cost = (output_tokens as u64 * pricing.completion_cost_per_1k) / 1000; prompt_cost.saturating_add(completion_cost)` — two-step computation matching RFC pseudocode structure
- [x] Uses `saturating_add` for local addition (single-request overflow is impossible; see note below)
- [x] Test vector: `let pricing = PricingModel { model_name: "test".into(), prompt_cost_per_1k: 30_000, completion_cost_per_1k: 60_000 }; assert_eq!(compute_cost(&pricing, 100, 50), 6000);`
- [x] Truncation note documented: error bounded at <2 micro-units per event

## Implementation Notes

- Location: `crates/quota-router-core/src/keys/models.rs` or new `compute.rs` module
- `PricingModel` struct: `model_name: String`, `prompt_cost_per_1k: u64`, `completion_cost_per_1k: u64`
- Division is integer division (truncates toward zero). For micro-unit pricing, truncation occurs only when cost < 0.5 micro-units — effectively free. Error is bounded at <2 micro-units per event (H1).
- 1000 = TOKEN_SCALE (micro-units per token)
- `saturating_add`: local per-event cost computation cannot overflow (would require >1.8×10¹⁹ tokens in a single request). This differs from `record_spend` budget accumulation which uses checked arithmetic (per RFC-0909 §Overflow Safety). `record_spend()` is defined in RFC-0903 Final §record_spend (M1).
- `compute_cost` takes `&PricingModel` (RFC-0909's type), NOT `&PricingTable` from RFC-0910

## Reference

- RFC-0909 §Cost Calculation
- RFC-0909 §compute_cost
- RFC-0909 §Truncation Note
- RFC-0909 §Overflow Safety (checked_add vs saturating_add distinction)
- RFC-0201 §Integer Scaling (TOKEN_SCALE = 1000)

## Dependencies

- `serde = { version = "1.x", features = ["derive"] }` for Serialize/Deserialize derives on PricingModel (H1)

## Implementation Note

Implemented in `crates/quota-router-core/src/keys/models.rs` + `crates/quota-router-core/src/keys/mod.rs`:
- `PricingModel` struct in `models.rs`: `model_name`, `prompt_cost_per_1k`, `completion_cost_per_1k` (all u64 micro-units)
- `compute_cost` in `mod.rs`: two-step integer arithmetic with `saturating_mul`/`saturating_div` and `saturating_add`
- 5 unit tests covering TV1, zero tokens, input-only, output-only, large tokens
- All AC items satisfied

## Complexity

Low — straightforward integer arithmetic

---
**Mission Type:** Implementation
**Priority:** Critical
**Phase:** RFC-0909 Phase 1 Core

## Changelog

| Version | Date | Changes |
|---------|------|---------|
| v5 | 2026-04-20 | Implemented: PricingModel struct, compute_cost standalone function, all 6 AC items complete, 5 unit tests passing |
| v4 | 2026-04-20 | Round 3 adversarial review fixes: fix H1 (serde dependency fixed to `serde = { version = "1.x", features = ["derive"] }`); fix M1 (add record_spend cross-reference to RFC-0903 Final §record_spend) |
| v3 | 2026-04-20 | Round 2 adversarial review fixes: fix C1 (add Serialize/Deserialize derives to PricingModel); fix H1 (add truncation context — error <0.5 micro-units per step); fix H2 (show two-step computation matching RFC pseudocode); fix M1 (add RFC-0201 §Integer Scaling reference); fix M2 (add serde dependency); fix L1 (show actual assert_eq! test code) |
| v2 | 2026-04-20 | Round 1 adversarial review fixes: fix C1 (add PricingModel struct to acceptance criteria); fix H1 (add saturating_add vs checked arithmetic distinction note); fix M1 (explicit test assertion); fix M2 (add TOKEN_SCALE micro-unit note) |
| v1 | 2026-04-20 | Initial |
