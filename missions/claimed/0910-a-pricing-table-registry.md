# Mission: RFC-0910 Full Implementation — Pricing Table Registry

## Status

Completed (2026-04-27)

## RFC

RFC-0910: Pricing Table Registry

## Dependencies

- RFC-0903-B1 (schema) — Completed
- RFC-0903-C1 (schema) — Completed
- RFC-0126 (DCS) — Accepted

## Summary

Implement RFC-0910 Pricing Table Registry from scratch. Key components:
1. `compute_pricing_hash()` using DCS Entry 16 binary encoding (RFC-0126 Part 3)
2. `get_canonical_tokenizer()` tokenizer lookup with EXACT_TABLE and prefix fallback
3. Tokenizer assignments for all known models including o1-mini/o1-preview
4. `PricingTable` struct with versioned registration
5. `compute_cost()` function (canonical, delegated from RFC-0904)

## Acceptance Criteria

- [x] `compute_pricing_hash()` — DCS Entry 16 binary encoding, returns `[u8; 32]`
- [x] Test vector passes: test table → `4a065c51147d4730379d600c4a491778b98f66a8e381c5dfdf51f42052c32f60`
- [x] `get_canonical_tokenizer(model: &str) -> &'static str` — exact match + 4-char prefix fallback dispatch
- [x] Tokenizer assignments: o1-mini → `tiktoken-o200k_base`, o1-preview → `tiktoken-o200k_base`
- [x] `PricingTable` struct with `register()`, `get()`, `compute_pricing_hash()`
- [x] `PricingRegistry` struct with versioned registration
- [x] `compute_cost()` function (canonical, receives `&PricingTable`)
- [x] `cargo clippy --all-targets --all-features -- -D warnings` passes with zero warnings
- [x] `cargo test --lib` passes (158 tests)

## Implementation Notes

**File:** `crates/quota-router-core/src/pricing.rs`

**PricingTable struct (fields 1-8 for DCS Entry 16 hash, no 9th field):**
```rust
pub struct PricingTable {
    pub table_id: String,           // field 1
    pub version: u32,              // field 2
    pub provider: String,           // field 3
    pub model: String,             // field 4
    pub prompt_cost_per_1k: u64,   // field 5
    pub completion_cost_per_1k: u64, // field 6
    pub effective_from: i64,        // field 7
    pub metadata: BTreeMap<String, String>, // field 8 (metadata key "tokenizer_version_expiry" optional)
}
```

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

**compute_cost signature (canonical, receives &PricingTable):**
```rust
pub fn compute_cost(
    pricing: &PricingTable,
    input_tokens: u32,
    output_tokens: u32,
) -> Result<u64, CostError>;
```

## Claimant

@mmacedoeu
