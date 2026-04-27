# Mission: RFC-0904/0910 Cost Integration — Delegated compute_cost

## Status

Implemented (2026-04-27)

## RFC

RFC-0904: Real-Time Cost Tracking (Accepted v1.31)

## Dependencies

- Mission: RFC-0910 Full Implementation ✅ Completed

## Summary

Update RFC-0904 code to match v1.29 spec: `compute_cost` should delegate to RFC-0910's canonical implementation, and OCTO-W balance functions should align with the RFC spec.

## Acceptance Criteria

- [x] `compute_cost()` delegates to RFC-0910 canonical implementation (not own `saturating_mul/div`)
- [x] `CostError::Overflow` → `BudgetError::CostOverflow` conversion implemented
- [x] OCTO-W `deduct_octo_w(key_id: &[u8; 16], cost_amount: u64) -> Result<u64, KeyError>` implemented
- [x] `octo_w_balances` table schema defined in storage
- [x] `cargo clippy --all-targets --all-features -- -D warnings` passes with zero warnings
- [x] `cargo test --lib` passes (161 tests)

## Implementation Notes

**File:** `crates/quota-router-core/src/keys/mod.rs`, `crates/quota-router-core/src/balance.rs`

**compute_cost delegation:**
```rust
pub fn compute_cost(...) -> Result<u64, BudgetError> {
    rfc0910::compute_cost(pricing, input_tokens, output_tokens)
        .map_err(|e| match e { CostError::Overflow { .. } => BudgetError::CostOverflow })
}
```

**OCTO-W balance DDL:**
```sql
CREATE TABLE octo_w_balances (
    key_id TEXT NOT NULL UNIQUE,
    balance INTEGER NOT NULL DEFAULT 0,
    updated_at INTEGER NOT NULL
);
```
