# Mission: RFC-0910 Full Implementation — Pricing Table Registry

## Status

Open

## RFC

RFC-0910 v25 (Draft): Pricing Table Registry

**Note:** RFC-0910 is Draft, not Accepted. Implementation should not proceed until RFC is Accepted per BLUEPRINT.md rules. Mission created for planning and tracking purposes.

## Dependencies

- RFC-0903-B1 (schema) — Completed
- RFC-0903-C1 (schema) — Completed
- RFC-0126 (DCS) — Accepted

## Summary

Implement RFC-0910 Pricing Table Registry from scratch. No implementation currently exists. Key components:
1. `compute_pricing_hash()` using DCS Entry 16 binary encoding (RFC-0126 Part 3)
2. `get_canonical_tokenizer()` tokenizer lookup with EXACT_TABLE and prefix fallback
3. Tokenizer assignments for all known models including o1-mini/o1-preview
4. `PricingTable` struct with versioned registration

## Acceptance Criteria

- [ ] `compute_pricing_hash()` — DCS Entry 16 binary encoding, returns `[u8; 32]`
- [ ] Test vector passes: test table → `4a065c51147d4730379d600c4a491778b98f66a8e381c5dfdf51f42052c32f60`
- [ ] `get_canonical_tokenizer(model: &str) -> &'static str` — exact match + 4-char prefix fallback dispatch
- [ ] Tokenizer assignments: o1-mini → `tiktoken-o200k_base`, o1-preview → `tiktoken-o200k_base`
- [ ] `PricingTable` struct with `register()`, `get()`, `compute_pricing_hash()`
- [ ] `PricingModel` struct: model_pattern, input_cost_per_1k, output_cost_per_1k, currency, effective_from
- [ ] `compute_cost()` function (delegated from RFC-0904)
- [ ] `cargo clippy --all-targets --all-features -- -D warnings` passes with zero warnings
- [ ] `cargo test --lib` passes

## Implementation Notes

**Files:** New module `crates/quota-router-core/src/pricing.rs` (or similar)

**DCS Entry 16 binary encoding rules:**
- field_id||value in declaration order (1-8)
- strings: length-prefixed UTF-8 (u32_be length + bytes)
- integers: binary big-endian (u32_be for u32, u64_be for u64, i64_be for i64)
- BTreeMap: u32_be(count)||sorted key-value entries

**Tokenizer 4-char prefix dispatch:**
```
"gem-" → gemini family
"gpt-" → openai gpt family
"o1-m" → o1-mini
"o1-p" → o1-preview
"o1-"  → o1 (fallback for o1-pro, o1, etc.)
"o3-"  → o3 family
"clau" → claude family
```
